//! `#[plugin_api]` 属性宏 — 插件互调机制（ADR-0017）的 IDL 层
//!
//! 挂在 trait 上（如 `#[bedcode_plugin_api::plugin_api] trait ScheduleApi { ... }`），
//! 一次定义、三处生成：
//!
//! - 原样输出 trait（插件实现它 = 实现方）
//! - `<Trait>Dispatcher`：JSON-RPC 请求分派（解析请求 → 调 trait 方法 → 回响应），
//!   插件在 `activate()` 调 `register()` 订阅请求 topic，在 `on_message()` 调
//!   `dispatch::<Self>(msg)` 接线
//! - `<Trait>Client`：类型化调用客户端（构造请求 → `host-api-call` → 解码回复），
//!   `ApiClient::<Trait>::new("com.bedcode.scheduler")` 语义，宏生成专用 client 类型
//!
//! ## 构建期防漂移（spec §9.4）
//!
//! 宏在编译期读取 manifest（默认 `<CARGO_MANIFEST_DIR>/plugin.json`，`manifest = "path"`
//! 可指向对侧插件的清单；manifest 是单一真源，ADR-0005），将 trait 方法推导出的 api 清单
//! （`<manifest.id>.<method>`）与 manifest `api` 字段比对，不一致直接 `compile_error!`
//! 使构建失败。**判据按角色分岔，角色由 manifest 是否落在本 crate 的插件包内自动判定**：
//!
//! - **声明方**（manifest 属本插件包）：要求**精确集合相等**——trait 与 manifest 任一侧
//!   改动都在构建期暴露，杜绝互调契约漂移。
//! - **消费方**（manifest 属对侧插件包）：只要求 **trait ⊆ manifest**——调用对侧未声明的
//!   api 仍编译失败，但对侧新增 api 不再要求不消费它的插件跟着镜像（ADR 0017 修订）。
//!
//! ## trait 方法契约
//!
//! - 静态方法（无 `self` 接收者），与 `WasmPlugin` 风格一致
//! - 返回 `Result<T, E>`，`T: Serialize + DeserializeOwned`（`()` 合法，对应 null），
//!   `E: ToString`（错误经 JSON-RPC error 对象回传；`String` / `anyhow::Error` 均可）
//! - 参数类型须 `Serialize + DeserializeOwned`；单参时 params 直接为值，
//!   多参为数组，零参为 null
//! - 方法名默认即 api 名段（snake_case），可用 `#[api("override.name")]` 覆盖

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Attribute, Error, FnArg, ItemTrait, ReturnType, TraitItem, Type};

/// 单个互调方法（由 trait 方法推导）
struct ApiMethod {
    ident: syn::Ident,
    /// JSON-RPC method 字段与请求 topic 末段（`#[api("...")]` 可覆盖）
    method_name: String,
    /// 参数 (名, 类型)
    args: Vec<(syn::Ident, Type)>,
    /// 返回 `Result<T, E>` 的 T（client 反序列化 / dispatcher 序列化目标）
    ret_ty: Type,
}

/// 解析 `Result<T, E>` 返回类型，取 T
fn result_ok_ty(ty: &Type, span: proc_macro2::Span) -> syn::Result<Type> {
    let Type::Path(p) = ty else {
        return Err(Error::new(span, "plugin_api 方法必须返回 Result<T, E>"));
    };
    let seg = p
        .path
        .segments
        .last()
        .ok_or_else(|| Error::new(span, "empty path"))?;
    if seg.ident != "Result" {
        return Err(Error::new(
            span,
            "plugin_api 方法必须返回 Result<T, E>（错误类型实现 ToString）",
        ));
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(Error::new(span, "Result 缺少泛型参数"));
    };
    let first = args
        .args
        .first()
        .ok_or_else(|| Error::new(span, "Result 缺少成功类型参数"))?;
    match first {
        syn::GenericArgument::Type(t) => Ok(t.clone()),
        _ => Err(Error::new(span, "Result 成功类型参数必须是类型")),
    }
}

/// 从方法属性中取 `#[api("name")]` 覆盖名
///
/// 文档形态为括号列表 `#[api("override.name")]`（Meta::List，括号内单个
/// 字符串字面量）；兼容 `#[api = "override.name"]`（Meta::NameValue）。
/// 2026-09-19 票 09 修复：此前仅支持 NameValue，括号形态（文档所载）解析
/// 为 List 永远不命中——覆盖名退化为 snake_case ident。
fn method_name_override(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("api") {
            continue;
        }
        match &attr.meta {
            // 文档形态：#[api("override.name")]
            syn::Meta::List(_) => {
                if let Ok(s) = attr.parse_args::<syn::LitStr>() {
                    return Some(s.value());
                }
            }
            // 兼容形态：#[api = "override.name"]
            syn::Meta::NameValue(nv) => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    return Some(s.value());
                }
            }
            _ => {}
        }
    }
    None
}

/// 解析 trait 定义 → 方法清单；并从输出 trait 中剥离 `#[api(...)]` 元属性
/// （仅宏自身消费，编译器不认识该属性）
fn parse_methods(trait_item: &mut ItemTrait) -> syn::Result<Vec<ApiMethod>> {
    if !trait_item.generics.params.is_empty() {
        return Err(Error::new(
            trait_item.generics.span(),
            "plugin_api trait 暂不支持泛型参数",
        ));
    }
    let mut methods = Vec::new();
    for item in &mut trait_item.items {
        let TraitItem::Fn(f) = item else {
            return Err(Error::new(
                item.span(),
                "plugin_api trait 仅支持方法定义",
            ));
        };
        if f.sig.receiver().is_some() {
            return Err(Error::new(
                f.sig.span(),
                "plugin_api 方法必须是静态方法（无 &self）",
            ));
        }
        // 参数：全部为 `name: Type` 具名参数
        let mut args = Vec::new();
        for input in &f.sig.inputs {
            match input {
                FnArg::Typed(pat) => match &*pat.pat {
                    syn::Pat::Ident(pid) => args.push((pid.ident.clone(), (*pat.ty).clone())),
                    _ => {
                        return Err(Error::new(
                            pat.span(),
                            "plugin_api 方法参数必须是具名参数",
                        ))
                    }
                },
                FnArg::Receiver(r) => {
                    return Err(Error::new(
                        r.span(),
                        "plugin_api 方法必须是静态方法（无 &self）",
                    ))
                }
            }
        }
        let ret = match &f.sig.output {
            ReturnType::Type(_, ty) => result_ok_ty(ty, f.sig.span())?,
            ReturnType::Default => {
                return Err(Error::new(
                    f.sig.span(),
                    "plugin_api 方法必须返回 Result<T, E>",
                ))
            }
        };
        // 先读 `#[api("override")]` 覆盖名再剥离元属性：覆盖名必须在 retain 之前
        // 读取（否则属性已被移除、覆盖永远不生效——2026-09-19 票 09 修复，附
        // parse_methods 回归测试 method_attr_override_participates_in_api_derivation）
        let method_name = method_name_override(&f.attrs).unwrap_or_else(|| f.sig.ident.to_string());
        f.attrs.retain(|a| !a.path().is_ident("api"));
        methods.push(ApiMethod {
            ident: f.sig.ident.clone(),
            method_name,
            args,
            ret_ty: ret,
        });
    }
    if methods.is_empty() {
        return Err(Error::new(
            trait_item.span(),
            "plugin_api trait 至少需要一个方法",
        ));
    }
    Ok(methods)
}

/// 读取并解析 manifest，返回 (id, api 数组)
fn read_manifest(path: &str) -> syn::Result<(String, Vec<String>)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "plugin_api: 读取 manifest '{}' 失败: {}。插件互调契约要求 manifest（plugin.json）\n\
                 作为单一真源（ADR-0005），请确认文件存在且含 id/api 字段。",
                path, e
            ),
        )
    })?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        Error::new(
            proc_macro2::Span::call_site(),
            format!("plugin_api: manifest '{}' 不是合法 JSON: {}", path, e),
        )
    })?;
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            Error::new(
                proc_macro2::Span::call_site(),
                format!("plugin_api: manifest '{}' 缺少字符串 id 字段", path),
            )
        })?
        .to_string();
    let apis = json
        .get("api")
        .map(|v| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or(Some(Vec::new()))
        .ok_or_else(|| {
            Error::new(
                proc_macro2::Span::call_site(),
                format!("plugin_api: manifest '{}' 的 api 字段必须是字符串数组", path),
            )
        })?;
    Ok((id, apis))
}

/// 防漂移比对角色：manifest 属于本插件包还是对侧插件包
///
/// 两种角色要防的是不同的错，判据因此不同：
///
/// - [`DriftRole::Declarant`]（声明方）：trait 即本插件对外 api 的实现。漏声明 = 实现了解不到、
///   多声明 = 调用必 404，两侧都是真缺陷，所以要求**精确集合相等**。
/// - [`DriftRole::Consumer`]（消费方）：trait 只是调用对侧 api 的 client 镜像。调对侧未声明的
///   api 必然运行期失败，所以要校 `trait ⊆ manifest`；而对侧**新增** api 与本插件无关，
///   精确相等会把「改 A 插件的 api」变成「不消费它的 B 插件一起编译红」（票 13 根因）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DriftRole {
    Declarant,
    Consumer,
}

/// 折叠路径中的 `.` 与可消除的 `..`（纯词法，不触碰文件系统）
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push(comp);
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// manifest 是否属于「本 crate 所在的插件包」
///
/// 判据：manifest 文件所在目录是本 crate 目录的自身或祖先。仓库两种真实形态都落在这条上：
/// 声明方写 `plugin.json`（同目录）或 `../plugin.json`（crate 在包的 `rust/` 子目录），
/// 消费方写 `../../<peer>/plugin.json`（目录分叉，不再是祖先）。
fn is_own_package_manifest(manifest_path: &std::path::Path, crate_dir: &std::path::Path) -> bool {
    let manifest_dir = match normalize_path(manifest_path).parent() {
        Some(dir) => dir.to_path_buf(),
        None => return false,
    };
    normalize_path(crate_dir).starts_with(&manifest_dir)
}

/// 判定角色；`CARGO_MANIFEST_DIR` 缺失时按消费方处理（判据更宽，不会误伤真实构建，
/// 且 `read_manifest` 随后仍会校验 manifest 可读）
fn drift_role(manifest_path: &std::path::Path, crate_dir: &str) -> DriftRole {
    if crate_dir.is_empty() {
        return DriftRole::Consumer;
    }
    if is_own_package_manifest(manifest_path, std::path::Path::new(crate_dir)) {
        DriftRole::Declarant
    } else {
        DriftRole::Consumer
    }
}

/// 构建期防漂移比对：trait 推导 api 清单 vs manifest api 字段（判据按 [`DriftRole`] 分岔）
fn check_drift(manifest_path: &str, methods: &[ApiMethod], role: DriftRole) -> syn::Result<String> {
    let (plugin_id, manifest_apis) = read_manifest(manifest_path)?;
    let mut trait_apis: Vec<String> = methods
        .iter()
        .map(|m| format!("{}.{}", plugin_id, m.method_name))
        .collect();
    trait_apis.sort();
    let mut manifest_sorted = manifest_apis.clone();
    manifest_sorted.sort();
    let missing: Vec<&String> = trait_apis
        .iter()
        .filter(|a| !manifest_sorted.contains(a))
        .collect();
    let extra: Vec<&String> = manifest_sorted
        .iter()
        .filter(|a| !trait_apis.contains(a))
        .collect();
    // 消费方只看 `missing`：对侧多声明的 api 不必镜像
    let drifts = !missing.is_empty() || (!extra.is_empty() && role == DriftRole::Declarant);
    if !drifts {
        return Ok(plugin_id);
    }
    let mut msg = format!(
        "plugin_api: trait 与 manifest '{}' 的 api 清单不一致（构建期防漂移检查失败，角色：{}）：\n",
        manifest_path,
        match role {
            DriftRole::Declarant => "声明方",
            DriftRole::Consumer => "消费方",
        }
    );
    if !missing.is_empty() {
        msg.push_str(&format!("  - trait 有但 manifest 缺: {:?}\n", missing));
    }
    if !extra.is_empty() {
        msg.push_str(&format!("  - manifest 有但 trait 缺: {:?}\n", extra));
    }
    msg.push_str(match role {
        DriftRole::Declarant => "  本 manifest 属本插件包（声明方）：trait 方法名 = manifest.api 条目（`<plugin-id>.<method>`），两侧必须精确一致",
        DriftRole::Consumer => "  本 manifest 属对侧插件包（消费方）：trait 须为 manifest.api 的子集，只镜像本插件真正调用的 api——对侧新增条目无需跟演",
    });
    Err(Error::new(proc_macro2::Span::call_site(), msg))
}

/// 生成单方法的参数反序列化代码（零参 → 占位；单参 → 值直取；多参 → 元组）
fn deser_code(m: &ApiMethod, api_full: &str) -> proc_macro2::TokenStream {
    let arg_pats: Vec<&syn::Ident> = m.args.iter().map(|(n, _)| n).collect();
    match m.args.len() {
        0 => quote! { let _ = &params; },
        1 => {
            let ty = &m.args[0].1;
            quote! {
                let #(#arg_pats)*: #ty = ::serde_json::from_value(params)
                    .map_err(|e| ::anyhow::anyhow!("plugin api '{}': invalid params: {}", #api_full, e))?;
            }
        }
        _ => {
            let tys: Vec<&Type> = m.args.iter().map(|(_, t)| t).collect();
            quote! {
                let (#(#arg_pats,)*): (#(#tys,)*) = ::serde_json::from_value(params)
                    .map_err(|e| ::anyhow::anyhow!("plugin api '{}': invalid params: {}", #api_full, e))?;
            }
        }
    }
}

/// 生成 client 方法的参数序列化表达式（零参 → null；单参 → 值；多参 → 元组数组）
fn params_expr(m: &ApiMethod) -> proc_macro2::TokenStream {
    match m.args.len() {
        0 => quote! { ::serde_json::Value::Null },
        1 => {
            let n = &m.args[0].0;
            quote! {
                ::serde_json::to_value(&#n)
                    .map_err(::bedcode_plugin_api::api_call::ApiCallError::serialize)?
            }
        }
        _ => {
            let idents: Vec<&syn::Ident> = m.args.iter().map(|(n, _)| n).collect();
            quote! {
                ::serde_json::to_value((#(&#idents),*))
                    .map_err(::bedcode_plugin_api::api_call::ApiCallError::serialize)?
            }
        }
    }
}

// ==================== 宏入口 ====================

/// 生成插件互调 trait 的实现方分派 + 调用方 client（见模块文档）
#[proc_macro_attribute]
pub fn plugin_api(attr: TokenStream, item: TokenStream) -> TokenStream {
    // 属性参数：`manifest = "path"`（相对 CARGO_MANIFEST_DIR，默认 plugin.json）
    let mut manifest_rel = "plugin.json".to_string();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("manifest") {
            let value = meta.value()?;
            let s: syn::LitStr = value.parse()?;
            manifest_rel = s.value();
            return Ok(());
        }
        Err(meta.error("plugin_api 仅支持 manifest = \"path\" 属性"))
    });
    parse_macro_input!(attr with parser);

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let manifest_path_buf = std::path::Path::new(&crate_dir).join(&manifest_rel);
    let manifest_path = manifest_path_buf.to_string_lossy().to_string();
    // 声明方 / 消费方由 manifest 是否落在本 crate 的插件包内决定（无需调用点再声明一层词汇）
    let role = drift_role(&manifest_path_buf, &crate_dir);

    let mut trait_item: ItemTrait = parse_macro_input!(item as ItemTrait);
    let trait_name = trait_item.ident.clone();
    let vis = trait_item.vis.clone();

    // 1. 方法清单 + 防漂移比对（比对失败 → compile_error，构建失败）
    let (methods, plugin_id) = match parse_methods(&mut trait_item)
        .and_then(|m| check_drift(&manifest_path, &m, role).map(|id| (m, id)))
    {
        Ok(ok) => ok,
        Err(e) => return e.to_compile_error().into(),
    };

    let dispatcher_name = format_ident!("{}Dispatcher", trait_name);
    let client_name = format_ident!("{}Client", trait_name);
    let api_names: Vec<String> = methods
        .iter()
        .map(|m| format!("{}.{}", plugin_id, m.method_name))
        .collect();

    // 2. 实现方分派：每个方法一个 match arm（topic 已定位 api，method 字段定方法）
    let mut dispatch_arms = Vec::new();
    for m in &methods {
        let api_full = format!("{}.{}", plugin_id, m.method_name);
        let method_lit = m.method_name.as_str();
        let method_ident = &m.ident;
        let arg_pats: Vec<&syn::Ident> = m.args.iter().map(|(n, _)| n).collect();
        let deser = deser_code(m, &api_full);
        dispatch_arms.push(quote! {
            #method_lit => {
                let params = req.params.clone().unwrap_or(::serde_json::Value::Null);
                #deser
                // 方法结果 → JSON-RPC 响应（result | error 二选一，错误经 error 对象回传）
                match <T as #trait_name>::#method_ident(#(#arg_pats),*) {
                    ::std::result::Result::Ok(v) => {
                        let v = ::serde_json::to_value(v).map_err(|e| {
                            ::anyhow::anyhow!("plugin api '{}': serialize result failed: {}", #api_full, e)
                        })?;
                        ::bedcode_plugin_api::api_call::rpc_reply(&req.id, ::std::result::Result::Ok(v))
                    }
                    ::std::result::Result::Err(e) => {
                        ::bedcode_plugin_api::api_call::rpc_reply(
                            &req.id,
                            ::std::result::Result::Err((-32000, ::std::string::ToString::to_string(&e))),
                        )
                    }
                }
            }
        });
    }

    // 3. 调用方 client：typed 方法（参数序列化 / 结果反序列化）
    let mut client_methods = Vec::new();
    for m in &methods {
        let method_ident = &m.ident;
        let method_lit = m.method_name.as_str();
        let ret_ty = &m.ret_ty;
        let arg_defs: Vec<proc_macro2::TokenStream> = m
            .args
            .iter()
            .map(|(n, t)| quote! { #n: #t })
            .collect();
        let params = params_expr(m);
        client_methods.push(quote! {
            /// 类型化调用（参数序列化 / 结果反序列化由宏生成）
            pub fn #method_ident(
                &self,
                #(#arg_defs),*
            ) -> ::std::result::Result<#ret_ty, ::bedcode_plugin_api::api_call::ApiCallError> {
                let params = #params;
                let reply = self.call_json(#method_lit, params)?;
                ::serde_json::from_value(reply)
                    .map_err(::bedcode_plugin_api::api_call::ApiCallError::decode_result)
            }
        });
    }

    let gen = quote! {
        #trait_item

        // ==================== 实现方：JSON-RPC 分派 ====================

        #vis struct #dispatcher_name;

        impl #dispatcher_name {
            /// 本 trait 声明的全部互调 api（全限定名，与 manifest.api 构建期比对一致）
            pub const API_NAMES: &'static [&'static str] = &[#(#api_names),*];

            /// 订阅全部请求 topic（`bedcode.api.<api>`）。插件在 activate() 中调用一次；
            /// 宿主订阅去重，重复调用幂等。
            pub fn register() -> ::std::result::Result<(), ::bedcode_plugin_api::host::HostError> {
                let host = ::bedcode_plugin_api::wasm_host::WasmHost;
                for api in Self::API_NAMES {
                    let topic = ::std::format!(
                        "{}{}",
                        ::bedcode_plugin_api::api_call::API_TOPIC_PREFIX,
                        api
                    );
                    host.bus_subscribe(&topic)?;
                }
                ::std::result::Result::Ok(())
            }

            /// 分发互调请求：topic 命中本 api 清单才处理并回响应，返回 true；
            /// 其余消息不处理返回 false（保持总线消息原有语义，不吞其他 topic）。
            ///
            /// 未实现 method（正常构建期防漂移不可能发生，仅在外部绕过声明注册
            /// 时出现）静默不回复，调用方按超时处理 —— spec 验收「模拟无响应目标」。
            pub fn dispatch<T: #trait_name>(
                msg: &::bedcode_plugin_api::BusMessage,
            ) -> ::anyhow::Result<bool> {
                let Some(api) = msg.topic
                    .strip_prefix(::bedcode_plugin_api::api_call::API_TOPIC_PREFIX)
                else {
                    return ::std::result::Result::Ok(false);
                };
                if !Self::API_NAMES.contains(&api) {
                    return ::std::result::Result::Ok(false);
                }
                // JSON-RPC 请求解析失败视为协议错误：仅记录日志不回复，防恶意载荷刷回复
                let req: ::bedcode_plugin_api::api_call::RpcRequest = match ::serde_json::from_value(msg.payload.clone()) {
                    ::std::result::Result::Ok(r) => r,
                    ::std::result::Result::Err(e) => {
                        let host = ::bedcode_plugin_api::wasm_host::WasmHost;
                        ::bedcode_plugin_api::host::HostLog::log_warn(
                            &host,
                            &::std::format!(
                                "plugin api dispatch: invalid JSON-RPC request on '{}': {}",
                                msg.topic, e
                            ),
                        );
                        return ::std::result::Result::Ok(true);
                    }
                };
                let reply = match req.method.as_str() {
                    #(#dispatch_arms)*
                    _ => {
                        // 未实现的 method：不回复（调用方超时）
                        return ::std::result::Result::Ok(false);
                    }
                };
                // 响应 topic：bedcode.api.reply.<caller-plugin-id>.<request-id>（spec §9.3）
                let reply_topic = ::std::format!(
                    "{}{}.{}",
                    ::bedcode_plugin_api::api_call::REPLY_TOPIC_PREFIX,
                    msg.sender,
                    req.id
                );
                ::bedcode_plugin_api::api_call::publish_reply(&reply_topic, &reply)?;
                ::std::result::Result::Ok(true)
            }
        }

        // ==================== 调用方：类型化 client ====================

        #vis struct #client_name {
            /// 目标插件 ID（请求 topic 前缀，如 com.bedcode.scheduler）
            target_id: ::std::string::String,
            /// 单次调用超时（毫秒，默认 10s）
            timeout_ms: u64,
        }

        impl #client_name {
            /// 创建指向目标插件的调用客户端（目标插件须已激活且声明对应 api）
            pub fn new(target_id: &str) -> Self {
                Self {
                    target_id: target_id.to_string(),
                    timeout_ms: ::bedcode_plugin_api::api_call::DEFAULT_CALL_TIMEOUT_MS,
                }
            }

            /// 覆盖单次调用超时（毫秒；默认 10s，spec §9.3）
            pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
                self.timeout_ms = timeout_ms;
                self
            }

            /// 通用 JSON-RPC 调用（typed 方法未覆盖的场景；返回值 = result 字段）
            pub fn call_json(
                &self,
                method: &str,
                params: ::serde_json::Value,
            ) -> ::std::result::Result<::serde_json::Value, ::bedcode_plugin_api::api_call::ApiCallError> {
                let request_topic = ::std::format!(
                    "{}{}.{}",
                    ::bedcode_plugin_api::api_call::API_TOPIC_PREFIX,
                    self.target_id,
                    method
                );
                let payload = ::bedcode_plugin_api::api_call::build_request(method, params);
                let reply = ::bedcode_plugin_api::api_call::api_call(
                    &request_topic,
                    &payload,
                    self.timeout_ms,
                )?;
                ::bedcode_plugin_api::api_call::decode_reply(&reply)
            }

            #(#client_methods)*
        }
    };

    gen.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 ApiMethod（仅 method_name 参与比对）
    fn method(name: &str) -> ApiMethod {
        ApiMethod {
            ident: syn::Ident::new(name, proc_macro2::Span::call_site()),
            method_name: name.to_string(),
            args: vec![],
            ret_ty: syn::parse_str::<Type>("::std::result::Result<(), String>").unwrap(),
        }
    }

    fn write_manifest(dir: &std::path::Path, id: &str, apis: &[&str]) -> String {
        let json = serde_json::json!({ "id": id, "api": apis });
        let path = dir.join("plugin.json");
        std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        path.to_string_lossy().to_string()
    }

    /// 防漂移通过：trait 方法 = manifest api 条目（id 前缀 + 方法名）
    #[test]
    fn drift_ok_when_sets_match() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_ok_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(&dir, "com.bedcode.test", &["com.bedcode.test.list", "com.bedcode.test.add"]);
        let id = check_drift(&path, &[method("add"), method("list")], DriftRole::Declarant)
            .expect("must pass");
        assert_eq!(id, "com.bedcode.test");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 防漂移失败：trait 改了方法但 manifest 没改 → 构建失败
    #[test]
    fn drift_fails_when_trait_method_missing_in_manifest() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_miss_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(&dir, "com.bedcode.test", &["com.bedcode.test.list"]);
        let err = check_drift(&path, &[method("add"), method("list")], DriftRole::Declarant)
            .unwrap_err();
        assert!(err.to_string().contains("trait 有但 manifest 缺"), "got: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 防漂移失败：manifest 声明了 trait 没有的 api（改 manifest 不改 trait → 构建失败）
    #[test]
    fn drift_fails_when_manifest_api_extra() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_extra_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(&dir, "com.bedcode.test", &["com.bedcode.test.list", "com.bedcode.test.remove"]);
        let err = check_drift(&path, &[method("list")], DriftRole::Declarant).unwrap_err();
        assert!(err.to_string().contains("manifest 有但 trait 缺"), "got: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 消费方判据（票 13）：对侧新增 api 不要求镜像 → 子集即通过
    #[test]
    fn consumer_passes_on_trait_subset() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_subset_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(
            &dir,
            "com.bedcode.test",
            &["com.bedcode.test.list", "com.bedcode.test.add", "com.bedcode.test.remove"],
        );
        let id = check_drift(&path, &[method("list")], DriftRole::Consumer).expect("subset must pass for consumer");
        assert_eq!(id, "com.bedcode.test");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 消费方判据仍守住的方向：调对侧未声明（改名/删除）的 api → 构建失败
    #[test]
    fn consumer_fails_when_calling_undeclared_api() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_conmiss_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(&dir, "com.bedcode.test", &["com.bedcode.test.list"]);
        let err = check_drift(&path, &[method("list"), method("renamed_away")], DriftRole::Consumer)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("trait 有但 manifest 缺"), "got: {}", msg);
        assert!(msg.contains("消费方"), "角色须点明，便于定位判据: {}", msg);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 角色判定：manifest 与本 crate 同包 / 在包的父目录 → 声明方；目录分叉 → 消费方
    #[test]
    fn role_is_own_package_only_within_the_same_package() {
        // 仓库两种声明方形态：manifest 与 crate 同目录、或在 crate 的父目录（插件包根）
        assert!(is_own_package_manifest(
            std::path::Path::new("/p/plugins/foo/plugin.json"),
            std::path::Path::new("/p/plugins/foo/rust")
        ));
        assert!(is_own_package_manifest(
            std::path::Path::new("/p/plugins/foo/rust/plugin.json"),
            std::path::Path::new("/p/plugins/foo/rust")
        ));
        // 相对形态（宏入口用 CARGO_MANIFEST_DIR join，故这里给已拼接的路径）
        assert!(is_own_package_manifest(
            std::path::Path::new("/p/plugins/foo/rust/../plugin.json"),
            std::path::Path::new("/p/plugins/foo/rust")
        ));
        // 对侧插件的清单：file-transfer 的 client 镜像 terminal-session
        assert!(!is_own_package_manifest(
            std::path::Path::new("/p/plugins/terminal-session/plugin.json"),
            std::path::Path::new("/p/plugins/file-transfer/rust")
        ));
        // 兄弟包前缀相似不得误判（`foo` 与 `foo-bar`）
        assert!(!is_own_package_manifest(
            std::path::Path::new("/p/plugins/foo-bar/plugin.json"),
            std::path::Path::new("/p/plugins/foo/rust")
        ));
    }

    /// 缺失 manifest：报错而非 panic（要求插件遵守 ADR-0005 单一真源）
    #[test]
    fn missing_manifest_errors_cleanly() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_none_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = check_drift(
            &dir.join("plugin.json").to_string_lossy(),
            &[method("list")],
            DriftRole::Declarant,
        )
        .unwrap_err();
        assert!(err.to_string().contains("读取 manifest"), "got: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `#[api("override")]` 覆盖名参与 api 推导
    #[test]
    fn method_name_override_respected() {
        let dir = std::env::temp_dir().join(format!("bedcode_macro_ovr_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_manifest(&dir, "com.bedcode.test", &["com.bedcode.test.schedule.list"]);
        let mut m = method("list");
        m.method_name = "schedule.list".to_string();
        check_drift(&path, &[m], DriftRole::Declarant).expect("override name must match manifest");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 回归（2026-09-19 票 09）：`#[api("...")]` 属性必须参与 `parse_methods` 的
    /// api 名推导——此前 retain 先于读取剥离属性，覆盖名永远不生效（方法名
    /// 只能退化为 snake_case ident）。本测试走真实 trait 解析路径。
    #[test]
    fn method_attr_override_participates_in_api_derivation() {
        let item: syn::ItemTrait = syn::parse_str(
            r#"
            trait Api {
                #[api("decide-consent")]
                fn decide_consent() -> Result<(), String>;
                fn plain() -> Result<(), String>;
            }
            "#,
        )
        .expect("parse trait");
        let mut item = item;
        let methods = parse_methods(&mut item).expect("parse methods");
        assert_eq!(methods.len(), 2);
        let decide = methods.iter().find(|m| m.ident == "decide_consent").expect("decide_consent");
        assert_eq!(
            decide.method_name, "decide-consent",
            "#[api] 覆盖名必须生效（retain 不得先于读取）"
        );
        let plain = methods.iter().find(|m| m.ident == "plain").expect("plain");
        assert_eq!(plain.method_name, "plain", "无覆盖时退化为方法 ident");
        // 覆盖属性已从输出 trait 剥离（编译器不认识 #[api]）
        let trait_str = quote::quote!(#item).to_string();
        assert!(!trait_str.contains("decide-consent"), "#[api] 属性必须剥离出输出 trait");
    }
}
