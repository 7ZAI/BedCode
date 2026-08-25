//! HTTP Traffic Filter Middleware
//!
//! 把 HTTP 请求体（入站）与响应体（出站）送入 [`TrafficFilterChain`] 责任链，
//! 以标准 actix Transform/Service 形式实现，由 `server/app.rs` 最内层 `.wrap()` 接入。
//!
//! 快速路径：链为空 / WS 升级握手 / HEAD → 不缓冲直接透传。
//!
//! 启用过滤器时，请求体与响应体会整体读入内存做转换——本服务当前均为
//! JSON 报文端点、无流式响应，整体缓冲可接受；未来新增大流量端点
//! （文件流下载等）时应由过滤器按 route 自行放行或扩展分块机制。

use std::rc::Rc;
use std::task::{Context, Poll};

use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{Payload, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorInternalServerError,
    http::header::CONTENT_LENGTH,
    web,
    web::Bytes,
    FromRequest, HttpResponse, Result,
};

use crate::server::dtos::common_dto::{ApiResponse, CODE_INVALID_REQUEST};
use crate::server::filter::{Direction, FilterContext, TrafficChannel, TrafficFilterChain};

// ==================== Transform（构造层） ====================

/// 流量过滤器中间件（HTTP 接入点）
///
/// 用法：`App::new().wrap(TrafficFilter)`。挂在最内层：CORS/日志层拒绝的
/// 请求不进入缓冲逻辑。
#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficFilter;

impl<S, B> Transform<S, ServiceRequest> for TrafficFilter
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse;
    type Error = actix_web::Error;
    type Transform = TrafficFilterService<S>;
    type InitError = ();
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(TrafficFilterService {
            service: Rc::new(service),
        }))
    }
}

// ==================== Service（执行层） ====================

pub struct TrafficFilterService<S> {
    /// Rc 包装：call() 需把下游服务移入 'static async 块（actix service 非 Send）
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for TrafficFilterService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse;
    type Error = actix_web::Error;
    type Future = futures_util::future::LocalBoxFuture<'static, Result<Self::Response>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // 快速路径在进入 async 块前判断，避免无谓的任务装箱开销：
        // WS 升级请求的帧级过滤在 WS 层做；HEAD 响应无 body；空链零介入
        let skip_filtering =
            req.path().starts_with("/ws") || req.method() == actix_web::http::Method::HEAD;

        let chain = TrafficFilterChain::global();
        if skip_filtering || chain.is_empty() {
            let fut = self.service.call(req);
            return Box::pin(async move { Ok(fut.await?.map_into_boxed_body()) });
        }

        let peer = req
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let path = req.path().to_string();
        let service = self.service.clone();

        Box::pin(async move { run_traffic_filter(req, service, peer, path).await })
    }
}

/// 完整过滤流程：入站请求体 → 下游 → 出站响应体
async fn run_traffic_filter<S, B>(
    req: ServiceRequest,
    service: Rc<S>,
    peer: String,
    path: String,
) -> Result<ServiceResponse>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    B: MessageBody + 'static,
{
    let chain = TrafficFilterChain::global();

    // ==================== 入站：请求体 ====================
    let (http_req, mut payload) = req.into_parts();
    let body = <web::Bytes as FromRequest>::from_request(&http_req, &mut payload).await?;

    let mut ctx = FilterContext {
        channel: TrafficChannel::Http,
        direction: Direction::Inbound,
        peer: &peer,
        route: &path,
        data: body.to_vec(),
    };
    if let Err(rej) = chain.run_inbound(&mut ctx) {
        tracing::warn!(peer = %peer, route = %path, %rej, "HTTP inbound request rejected by traffic filter");
        let response = HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            CODE_INVALID_REQUEST,
            &format!("Traffic filter rejected request: {rej}"),
        ));
        return Ok(ServiceResponse::new(http_req, response));
    }

    let service_req = ServiceRequest::from_parts(http_req, Payload::from(Bytes::from(ctx.data)));

    // ==================== 下游处理 ====================
    let res = service.call(service_req).await?.map_into_boxed_body();

    // ==================== 出站：响应体 ====================
    let (res_req, http_res) = res.into_parts();
    let (res_head, res_body) = http_res.into_parts();
    let res_bytes = to_bytes_owned(res_body).await?;

    let mut out_ctx = FilterContext {
        channel: TrafficChannel::Http,
        direction: Direction::Outbound,
        peer: &peer,
        route: &path,
        data: res_bytes,
    };
    if let Err(rej) = chain.run_outbound(&mut out_ctx) {
        tracing::warn!(peer = %peer, route = %path, %rej, "HTTP outbound response rejected by traffic filter");
        let response = HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            CODE_INVALID_REQUEST,
            &format!("Traffic filter rejected response: {rej}"),
        ));
        return Ok(ServiceResponse::new(res_req, response));
    }

    // 重建响应：替换体后 Content-Length 已失配，剔除原值由 builder 按新体重算；
    // 其余头（含 CORS / Set-Cookie / Content-Type）原样保留
    let mut builder = HttpResponse::build(res_head.status());
    for (name, value) in res_head.headers() {
        if name == CONTENT_LENGTH {
            continue;
        }
        builder.insert_header((name.clone(), value.clone()));
    }
    let rebuilt = builder.body(Bytes::from(out_ctx.data));

    Ok(ServiceResponse::new(res_req, rebuilt))
}

/// 缓冲完整响应体（BoxBody）为内存字节
async fn to_bytes_owned(body: BoxBody) -> Result<Vec<u8>> {
    let bytes = actix_web::body::to_bytes(body)
        .await
        .map_err(ErrorInternalServerError)?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::filter::{TrafficFilter, Verdict};
    use actix_web::{test, web, App};

    /// 全局链是进程级单例，串行化触碰它的集成测试
    static GLOBAL_CHAIN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 转换型过滤器：入站请求体转大写（模拟解密），出站响应体追加标记（模拟加密）
    struct TransformFilter;

    impl TrafficFilter for TransformFilter {
        fn name(&self) -> &str {
            "test-transform"
        }
        fn on_inbound(&self, ctx: &mut FilterContext<'_>) -> Verdict {
            ctx.data = ctx.data.to_ascii_uppercase();
            Verdict::Continue
        }
        fn on_outbound(&self, ctx: &mut FilterContext<'_>) -> Verdict {
            let mut body = String::from_utf8_lossy(&ctx.data).into_owned();
            body.push_str("|ENCRYPTED");
            ctx.data = body.into_bytes();
            Verdict::Continue
        }
    }

    /// 拒绝型过滤器：入站一律拒绝
    struct RejectAllFilter;

    impl TrafficFilter for RejectAllFilter {
        fn name(&self) -> &str {
            "reject-all"
        }
        fn on_inbound(&self, _ctx: &mut FilterContext<'_>) -> Verdict {
            Verdict::Reject("blocked by test".to_string())
        }
    }

    async fn echo_handler(body: web::Bytes) -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({ "received": String::from_utf8_lossy(&body) }))
    }

    #[actix_web::test]
    async fn request_and_response_bodies_pass_through_chain() {
        let _guard = GLOBAL_CHAIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let chain = TrafficFilterChain::global();
        chain.clear();
        chain.register(std::sync::Arc::new(TransformFilter));

        let app = test::init_service(
            App::new()
                .wrap(TrafficFilter)
                .route("/echo", web::post().to(echo_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/echo")
            .set_payload("hello")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(res.status().is_success());

        // 出站过滤器已改写响应体
        let body = test::read_body(res).await;
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("ENCRYPTED"), "响应体应经过出站过滤: {text}");
        assert!(
            text.contains("HELLO"),
            "handler 应收到入站过滤后的大写请求体: {text}"
        );

        chain.clear();
    }

    #[actix_web::test]
    async fn inbound_rejection_returns_400_without_reaching_handler() {
        let _guard = GLOBAL_CHAIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let chain = TrafficFilterChain::global();
        chain.clear();
        chain.register(std::sync::Arc::new(RejectAllFilter));

        let app = test::init_service(
            App::new()
                .wrap(TrafficFilter)
                .route("/echo", web::post().to(echo_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/echo")
            .set_payload("hello")
            .to_request();
        let res = test::call_service(&app, req).await;

        assert_eq!(res.status(), actix_web::http::StatusCode::BAD_REQUEST);
        let body = test::read_body(res).await;
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("reject-all") && text.contains("blocked by test"),
            "错误响应应携带拒绝详情: {text}"
        );

        chain.clear();
    }

    #[actix_web::test]
    async fn empty_chain_passthrough_keeps_response_intact() {
        let _guard = GLOBAL_CHAIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        TrafficFilterChain::global().clear();

        let app = test::init_service(
            App::new()
                .wrap(TrafficFilter)
                .route("/echo", web::post().to(echo_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/echo")
            .set_payload("hello")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(res.status().is_success());

        let body = test::read_body(res).await;
        assert!(!String::from_utf8_lossy(&body).contains("ENCRYPTED"));
    }
}
