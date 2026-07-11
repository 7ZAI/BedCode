const { defineComponent, computed, openBlock, createElementBlock, normalizeClass, createCommentVNode, createElementVNode, toDisplayString, ref, withDirectives, withKeys, withModifiers, vModelText, unref, nextTick, Fragment, renderList, createTextVNode, reactive, watch, vModelDynamic, vModelSelect, createVNode, createBlock, inject, onMounted } = window.__BEDCODE_SHARED__["vue"];
const { useI18n } = window.__BEDCODE_SHARED__["vue-i18n"];
const _hoisted_1$7 = {
  key: 0,
  class: "w-7 h-7 rounded-full bg-brand-light flex items-center justify-center text-xs flex-shrink-0 mt-1"
};
const _hoisted_2$7 = {
  key: 0,
  class: "flex items-center gap-1"
};
const _hoisted_3$7 = ["innerHTML"];
const _hoisted_4$6 = {
  key: 2,
  class: "whitespace-pre-wrap"
};
const _hoisted_5$6 = {
  key: 1,
  class: "w-7 h-7 rounded-full bg-[var(--bg-hover)] flex items-center justify-center text-xs flex-shrink-0 mt-1"
};
const _sfc_main$7 = /* @__PURE__ */ defineComponent({
  __name: "ChatMessage",
  props: {
    message: {},
    streaming: { type: Boolean }
  },
  setup(__props) {
    const props = __props;
    function escapeHtml(text) {
      return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#039;");
    }
    function renderMarkdown(escaped) {
      let text = escaped;
      text = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
        return `<pre class="bg-[var(--bg-code)] text-[var(--text-code)] rounded p-2 my-1 overflow-x-auto text-xs"><code>${code}</code></pre>`;
      });
      text = text.replace(/`([^`]+)`/g, '<code class="bg-[var(--bg-hover)] px-1 rounded text-xs">$1</code>');
      text = text.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
      text = text.replace(/\*([^*]+)\*/g, "<em>$1</em>");
      text = text.replace(/\n/g, "<br>");
      return text;
    }
    const renderedContent = computed(() => {
      const escaped = escapeHtml(props.message.content);
      return renderMarkdown(escaped);
    });
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", {
        class: normalizeClass(["flex gap-3", __props.message.role === "user" ? "justify-end" : "justify-start"])
      }, [
        __props.message.role === "assistant" ? (openBlock(), createElementBlock("div", _hoisted_1$7, " AI ")) : createCommentVNode("", true),
        createElementVNode("div", {
          class: normalizeClass([
            "max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed",
            __props.message.role === "user" ? "bg-brand text-white" : "bg-[var(--bg-hover)] text-[var(--text-primary)]"
          ])
        }, [
          __props.message.role === "assistant" && !__props.message.content && __props.streaming ? (openBlock(), createElementBlock("div", _hoisted_2$7, [..._cache[0] || (_cache[0] = [
            createElementVNode("span", { class: "inline-block w-1.5 h-4 bg-brand animate-pulse" }, null, -1)
          ])])) : __props.message.role === "assistant" ? (openBlock(), createElementBlock("div", {
            key: 1,
            innerHTML: renderedContent.value
          }, null, 8, _hoisted_3$7)) : (openBlock(), createElementBlock("div", _hoisted_4$6, toDisplayString(__props.message.content), 1))
        ], 2),
        __props.message.role === "user" ? (openBlock(), createElementBlock("div", _hoisted_5$6, toDisplayString(_ctx.$t("desktop.plugin.aiChatbox.send").charAt(0)), 1)) : createCommentVNode("", true)
      ], 2);
    };
  }
});
const _hoisted_1$6 = { class: "flex gap-2 items-end" };
const _hoisted_2$6 = ["placeholder", "disabled", "onKeydown"];
const _hoisted_3$6 = ["disabled"];
const _sfc_main$6 = /* @__PURE__ */ defineComponent({
  __name: "ChatInput",
  props: {
    disabled: { type: Boolean, default: false },
    placeholder: { default: "" }
  },
  emits: ["send"],
  setup(__props, { emit: __emit }) {
    const { t } = useI18n();
    const props = __props;
    const label = t("desktop.plugin.aiChatbox.send");
    const emit = __emit;
    const text = ref("");
    const inputRef = ref(null);
    function handleSend() {
      const content = text.value.trim();
      if (!content || props.disabled) return;
      emit("send", content);
      text.value = "";
      nextTick(() => autoResize());
    }
    function autoResize() {
      const el = inputRef.value;
      if (!el) return;
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 120) + "px";
    }
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1$6, [
        withDirectives(createElementVNode("textarea", {
          ref_key: "inputRef",
          ref: inputRef,
          "onUpdate:modelValue": _cache[0] || (_cache[0] = ($event) => text.value = $event),
          placeholder: __props.placeholder,
          disabled: __props.disabled,
          rows: "1",
          class: "flex-1 resize-none bg-[var(--bg-card)] border border-[var(--border)] rounded-input px-3 py-2 text-sm text-[var(--text-primary)] placeholder-[var(--text-tertiary)] focus:border-brand outline-none",
          onKeydown: withKeys(withModifiers(handleSend, ["exact", "prevent"]), ["enter"]),
          onInput: autoResize
        }, null, 40, _hoisted_2$6), [
          [vModelText, text.value]
        ]),
        createElementVNode("button", {
          disabled: __props.disabled || !text.value.trim(),
          class: "px-3 py-2 bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-btn text-sm font-medium transition-colors flex-shrink-0",
          onClick: handleSend
        }, toDisplayString(unref(label)), 9, _hoisted_3$6)
      ]);
    };
  }
});
const SHARED_KEY = "__BEDCODE_SHARED__";
function getSharedModule(name) {
  const shared = window[SHARED_KEY];
  if (!shared) throw new Error("[PluginSDK] Shared runtime not initialized");
  const mod = shared[name];
  if (!mod) throw new Error(`[PluginSDK] Shared module "${name}" not found`);
  return mod;
}
function getI18n() {
  return getSharedModule("i18n");
}
const PROVIDER_PRESETS = [
  { name: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", apiFormat: "openai", models: ["deepseek-chat", "deepseek-reasoner"] },
  { name: "OpenAI", baseUrl: "https://api.openai.com/v1", apiFormat: "openai", models: ["gpt-4o-mini", "gpt-4o", "gpt-4-turbo"] },
  { name: "Anthropic", baseUrl: "https://api.anthropic.com", apiFormat: "anthropic", models: ["claude-sonnet-4-20250514", "claude-haiku-4-20250414"] },
  { name: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta", apiFormat: "gemini", models: ["gemini-2.0-flash", "gemini-1.5-pro"] },
  { name: "通义千问", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", apiFormat: "openai", models: ["qwen-turbo", "qwen-plus", "qwen-max"] },
  { name: "Moonshot", baseUrl: "https://api.moonshot.cn/v1", apiFormat: "openai", models: ["moonshot-v1-8k", "moonshot-v1-32k"] },
  { name: "智谱", baseUrl: "https://open.bigmodel.cn/api/paas/v4", apiFormat: "openai", models: ["glm-4-flash", "glm-4-plus", "glm-4"] },
  { name: "硅基流动", baseUrl: "https://api.siliconflow.cn/v1", apiFormat: "openai", models: ["Qwen/Qwen2.5-7B-Instruct", "deepseek-ai/DeepSeek-V3"] },
  { name: "Ollama", baseUrl: "http://localhost:11434", apiFormat: "ollama", models: [] }
];
const API_FORMAT_OPTIONS = [
  { value: "openai", label: "OpenAI API" },
  { value: "anthropic", label: "Anthropic Messages" },
  { value: "gemini", label: "Google Gemini" },
  { value: "ollama", label: "Ollama" }
];
function generateId() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}
const _hoisted_1$5 = { class: "h-full flex flex-col bg-[var(--bg-page)] border-r border-[var(--border)]" };
const _hoisted_2$5 = { class: "p-3" };
const _hoisted_3$5 = { class: "px-2 mb-2 text-xs font-medium text-[var(--text-tertiary)] uppercase tracking-wider" };
const _hoisted_4$5 = { class: "space-y-0.5" };
const _hoisted_5$5 = ["onClick"];
const _hoisted_6$4 = { class: "truncate" };
const _hoisted_7$3 = { class: "flex-1 p-3 overflow-y-auto" };
const _hoisted_8$3 = { class: "px-2 mb-2 text-xs font-medium text-[var(--text-tertiary)] uppercase tracking-wider" };
const _hoisted_9$3 = {
  key: 0,
  class: "space-y-0.5"
};
const _hoisted_10$3 = ["onClick"];
const _hoisted_11$3 = { class: "truncate" };
const _hoisted_12$3 = { class: "text-xs text-[var(--text-tertiary)] shrink-0 ml-1" };
const _hoisted_13$3 = {
  key: 1,
  class: "px-2 text-xs text-[var(--text-tertiary)]"
};
const _hoisted_14$3 = { class: "p-3 border-t border-[var(--border)]" };
const _sfc_main$5 = /* @__PURE__ */ defineComponent({
  __name: "ProviderSidebar",
  props: {
    providers: {},
    selectedProviderId: {},
    selectedPresetName: {},
    isAddMode: { type: Boolean }
  },
  emits: ["selectProvider", "selectPreset", "addNew"],
  setup(__props, { emit: __emit }) {
    const { t } = useI18n();
    const emit = __emit;
    const presets = PROVIDER_PRESETS;
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1$5, [
        createElementVNode("div", _hoisted_2$5, [
          createElementVNode("h4", _hoisted_3$5, toDisplayString(unref(t)("desktop.plugin.aiChatbox.presetProviders")), 1),
          createElementVNode("ul", _hoisted_4$5, [
            (openBlock(true), createElementBlock(Fragment, null, renderList(unref(presets), (preset) => {
              return openBlock(), createElementBlock("li", {
                key: preset.name
              }, [
                createElementVNode("button", {
                  class: normalizeClass([
                    "w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-sm transition-colors text-left",
                    __props.selectedPresetName === preset.name ? "bg-brand-light text-[var(--text-brand)]" : "text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]"
                  ]),
                  onClick: ($event) => emit("selectPreset", preset)
                }, [
                  createElementVNode("span", _hoisted_6$4, toDisplayString(preset.name), 1)
                ], 10, _hoisted_5$5)
              ]);
            }), 128))
          ])
        ]),
        _cache[2] || (_cache[2] = createElementVNode("div", { class: "mx-3 border-t border-[var(--border)]" }, null, -1)),
        createElementVNode("div", _hoisted_7$3, [
          createElementVNode("h4", _hoisted_8$3, toDisplayString(unref(t)("desktop.plugin.aiChatbox.customProviders")), 1),
          __props.providers.length > 0 ? (openBlock(), createElementBlock("ul", _hoisted_9$3, [
            (openBlock(true), createElementBlock(Fragment, null, renderList(__props.providers, (provider) => {
              return openBlock(), createElementBlock("li", {
                key: provider.id
              }, [
                createElementVNode("button", {
                  class: normalizeClass([
                    "w-full flex items-center justify-between px-2 py-1.5 rounded-md text-sm transition-colors text-left",
                    __props.selectedProviderId === provider.id ? "bg-brand-light text-[var(--text-brand)]" : "text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]"
                  ]),
                  onClick: ($event) => emit("selectProvider", provider.id)
                }, [
                  createElementVNode("span", _hoisted_11$3, toDisplayString(provider.name), 1),
                  createElementVNode("span", _hoisted_12$3, toDisplayString(provider.models.length), 1)
                ], 10, _hoisted_10$3)
              ]);
            }), 128))
          ])) : (openBlock(), createElementBlock("p", _hoisted_13$3, toDisplayString(unref(t)("desktop.plugin.aiChatbox.noProvider")), 1))
        ]),
        createElementVNode("div", _hoisted_14$3, [
          createElementVNode("button", {
            class: normalizeClass([
              "w-full flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors",
              __props.isAddMode ? "bg-brand-light text-[var(--text-brand)]" : "text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]"
            ]),
            onClick: _cache[0] || (_cache[0] = ($event) => emit("addNew"))
          }, [
            _cache[1] || (_cache[1] = createElementVNode("svg", {
              class: "w-4 h-4",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              createElementVNode("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M12 4v16m8-8H4"
              })
            ], -1)),
            createTextVNode(" " + toDisplayString(unref(t)("desktop.plugin.aiChatbox.addCustomProvider")), 1)
          ], 2)
        ])
      ]);
    };
  }
});
const _hoisted_1$4 = { class: "space-y-2" };
const _hoisted_2$4 = { class: "flex items-center justify-between" };
const _hoisted_3$4 = { class: "text-sm text-[var(--text-secondary)]" };
const _hoisted_4$4 = {
  key: 0,
  class: "text-xs text-[var(--text-tertiary)] py-2"
};
const _hoisted_5$4 = ["value", "placeholder", "onInput"];
const _hoisted_6$3 = ["onClick"];
const _sfc_main$4 = /* @__PURE__ */ defineComponent({
  __name: "ModelListEditor",
  props: {
    models: {}
  },
  emits: ["update"],
  setup(__props, { emit: __emit }) {
    const { t } = useI18n();
    const props = __props;
    const emit = __emit;
    function addModel() {
      emit("update", [...props.models, ""]);
    }
    function removeModel(index) {
      const updated = [...props.models];
      updated.splice(index, 1);
      emit("update", updated);
    }
    function updateModel(index, value) {
      const updated = [...props.models];
      updated[index] = value;
      emit("update", updated);
    }
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1$4, [
        createElementVNode("div", _hoisted_2$4, [
          createElementVNode("label", _hoisted_3$4, toDisplayString(unref(t)("desktop.plugin.aiChatbox.modelList")), 1),
          createElementVNode("button", {
            class: "flex items-center gap-1 px-2 py-1 text-xs rounded-md bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]/80 transition-colors",
            onClick: addModel
          }, [
            _cache[0] || (_cache[0] = createElementVNode("svg", {
              class: "w-3 h-3",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              createElementVNode("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M12 4v16m8-8H4"
              })
            ], -1)),
            createTextVNode(" " + toDisplayString(unref(t)("desktop.plugin.aiChatbox.addModel")), 1)
          ])
        ]),
        __props.models.length === 0 ? (openBlock(), createElementBlock("div", _hoisted_4$4, toDisplayString(unref(t)("desktop.plugin.aiChatbox.noModels")), 1)) : createCommentVNode("", true),
        (openBlock(true), createElementBlock(Fragment, null, renderList(__props.models, (model, index) => {
          return openBlock(), createElementBlock("div", {
            key: index,
            class: "flex items-center gap-2"
          }, [
            createElementVNode("input", {
              value: model,
              type: "text",
              placeholder: unref(t)("desktop.plugin.aiChatbox.modelId"),
              class: "flex-1 bg-[var(--bg-card)] border border-[var(--border)] rounded-md px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors",
              onInput: ($event) => updateModel(index, $event.target.value)
            }, null, 40, _hoisted_5$4),
            createElementVNode("button", {
              class: "p-1 text-[var(--text-tertiary)] hover:text-[var(--color-danger)] transition-colors",
              onClick: ($event) => removeModel(index)
            }, [..._cache[1] || (_cache[1] = [
              createElementVNode("svg", {
                class: "w-4 h-4",
                fill: "none",
                stroke: "currentColor",
                viewBox: "0 0 24 24"
              }, [
                createElementVNode("path", {
                  "stroke-linecap": "round",
                  "stroke-linejoin": "round",
                  "stroke-width": "2",
                  d: "M6 18L18 6M6 6l12 12"
                })
              ], -1)
            ])], 8, _hoisted_6$3)
          ]);
        }), 128))
      ]);
    };
  }
});
const _hoisted_1$3 = { class: "h-full flex flex-col" };
const _hoisted_2$3 = { class: "mb-6" };
const _hoisted_3$3 = { class: "text-lg font-semibold text-[var(--text-primary)]" };
const _hoisted_4$3 = {
  key: 0,
  class: "mt-1 text-sm text-[var(--text-tertiary)]"
};
const _hoisted_5$3 = { class: "flex-1 space-y-5" };
const _hoisted_6$2 = { class: "block text-sm text-[var(--text-secondary)] mb-1.5" };
const _hoisted_7$2 = ["placeholder"];
const _hoisted_8$2 = {
  key: 0,
  class: "mt-1 text-xs text-[var(--color-danger)]"
};
const _hoisted_9$2 = { class: "block text-sm text-[var(--text-secondary)] mb-1.5" };
const _hoisted_10$2 = {
  key: 0,
  class: "mt-1 text-xs text-[var(--color-danger)]"
};
const _hoisted_11$2 = { class: "block text-sm text-[var(--text-secondary)] mb-1.5" };
const _hoisted_12$2 = { class: "relative" };
const _hoisted_13$2 = ["type", "placeholder"];
const _hoisted_14$2 = {
  key: 0,
  class: "w-4 h-4",
  fill: "none",
  stroke: "currentColor",
  viewBox: "0 0 24 24"
};
const _hoisted_15$2 = {
  key: 1,
  class: "w-4 h-4",
  fill: "none",
  stroke: "currentColor",
  viewBox: "0 0 24 24"
};
const _hoisted_16$1 = {
  key: 0,
  class: "mt-1 text-xs text-[var(--color-danger)]"
};
const _hoisted_17 = { class: "block text-sm text-[var(--text-secondary)] mb-1.5" };
const _hoisted_18 = ["value"];
const _hoisted_19 = {
  key: 0,
  class: "text-xs text-[var(--color-danger)]"
};
const _hoisted_20 = { class: "flex items-center gap-3 pt-6 mt-6 border-t border-[var(--border)]" };
const _sfc_main$3 = /* @__PURE__ */ defineComponent({
  __name: "ProviderForm",
  props: {
    mode: {},
    initialValues: {},
    existingNames: {}
  },
  emits: ["save", "delete"],
  setup(__props, { emit: __emit }) {
    var _a, _b, _c, _d, _e, _f;
    const { t } = useI18n();
    const i18n = getI18n();
    const props = __props;
    const emit = __emit;
    const apiFormatOptions = API_FORMAT_OPTIONS.map((opt) => ({
      ...opt,
      label: t(`desktop.plugin.aiChatbox.format${opt.value.charAt(0).toUpperCase() + opt.value.slice(1)}`)
    }));
    const showApiKey = ref(false);
    const editingId = ref(((_a = props.initialValues) == null ? void 0 : _a.id) || "");
    const form = reactive({
      name: ((_b = props.initialValues) == null ? void 0 : _b.name) || "",
      baseUrl: ((_c = props.initialValues) == null ? void 0 : _c.baseUrl) || "",
      apiKey: ((_d = props.initialValues) == null ? void 0 : _d.apiKey) || "",
      apiFormat: ((_e = props.initialValues) == null ? void 0 : _e.apiFormat) || "openai",
      models: ((_f = props.initialValues) == null ? void 0 : _f.models) ? [...props.initialValues.models] : []
    });
    const errors = reactive({});
    watch(() => props.initialValues, (val) => {
      if (val) {
        form.name = val.name;
        form.baseUrl = val.baseUrl;
        form.apiKey = val.apiKey;
        form.apiFormat = val.apiFormat;
        form.models = [...val.models];
        editingId.value = val.id;
      }
    }, { deep: true });
    function validate() {
      var _a2;
      errors.name = "";
      errors.baseUrl = "";
      errors.apiKey = "";
      errors.models = "";
      let valid = true;
      if (!form.name.trim()) {
        errors.name = i18n.global.t("desktop.plugin.aiChatbox.nameRequired");
        valid = false;
      } else if (props.mode === "add" && ((_a2 = props.existingNames) == null ? void 0 : _a2.includes(form.name.trim()))) {
        errors.name = i18n.global.t("desktop.plugin.aiChatbox.providerExists", { name: form.name });
        valid = false;
      }
      if (!form.baseUrl.trim()) {
        errors.baseUrl = i18n.global.t("desktop.plugin.aiChatbox.baseUrlRequired");
        valid = false;
      }
      if (form.apiFormat !== "ollama" && !form.apiKey.trim()) {
        errors.apiKey = i18n.global.t("desktop.plugin.aiChatbox.apiKeyRequired");
        valid = false;
      }
      const nonEmptyModels = form.models.filter((m) => m.trim());
      if (nonEmptyModels.length === 0) {
        errors.models = i18n.global.t("desktop.plugin.aiChatbox.modelRequired");
        valid = false;
      }
      return valid;
    }
    function handleSave() {
      var _a2;
      if (!validate()) return;
      const nonEmptyModels = form.models.filter((m) => m.trim());
      const provider = {
        id: props.mode === "edit" ? editingId.value : generateId(),
        name: form.name.trim(),
        apiKey: form.apiKey.trim(),
        baseUrl: form.baseUrl.trim(),
        apiFormat: form.apiFormat,
        models: nonEmptyModels,
        activeModel: ((_a2 = props.initialValues) == null ? void 0 : _a2.activeModel) || nonEmptyModels[0] || ""
      };
      emit("save", provider);
    }
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1$3, [
        createElementVNode("div", _hoisted_2$3, [
          createElementVNode("h2", _hoisted_3$3, toDisplayString(__props.mode === "add" ? unref(t)("desktop.plugin.aiChatbox.addProvider") : unref(t)("desktop.plugin.aiChatbox.editProvider")), 1),
          __props.mode === "add" ? (openBlock(), createElementBlock("p", _hoisted_4$3, toDisplayString(unref(t)("desktop.plugin.aiChatbox.subtitle")), 1)) : createCommentVNode("", true)
        ]),
        createElementVNode("div", _hoisted_5$3, [
          createElementVNode("div", null, [
            createElementVNode("label", _hoisted_6$2, toDisplayString(unref(t)("desktop.plugin.aiChatbox.name")), 1),
            withDirectives(createElementVNode("input", {
              "onUpdate:modelValue": _cache[0] || (_cache[0] = ($event) => form.name = $event),
              type: "text",
              placeholder: unref(t)("desktop.plugin.aiChatbox.name"),
              class: normalizeClass(["w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors", errors.name ? "border-[var(--color-danger)]" : "border-[var(--border)]"])
            }, null, 10, _hoisted_7$2), [
              [vModelText, form.name]
            ]),
            errors.name ? (openBlock(), createElementBlock("p", _hoisted_8$2, toDisplayString(errors.name), 1)) : createCommentVNode("", true)
          ]),
          createElementVNode("div", null, [
            createElementVNode("label", _hoisted_9$2, toDisplayString(unref(t)("desktop.plugin.aiChatbox.baseUrl")), 1),
            withDirectives(createElementVNode("input", {
              "onUpdate:modelValue": _cache[1] || (_cache[1] = ($event) => form.baseUrl = $event),
              type: "text",
              placeholder: "https://api.example.com/v1",
              class: normalizeClass(["w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors", errors.baseUrl ? "border-[var(--color-danger)]" : "border-[var(--border)]"])
            }, null, 2), [
              [vModelText, form.baseUrl]
            ]),
            errors.baseUrl ? (openBlock(), createElementBlock("p", _hoisted_10$2, toDisplayString(errors.baseUrl), 1)) : createCommentVNode("", true)
          ]),
          createElementVNode("div", null, [
            createElementVNode("label", _hoisted_11$2, toDisplayString(unref(t)("desktop.plugin.aiChatbox.apiKey")), 1),
            createElementVNode("div", _hoisted_12$2, [
              withDirectives(createElementVNode("input", {
                "onUpdate:modelValue": _cache[2] || (_cache[2] = ($event) => form.apiKey = $event),
                type: showApiKey.value ? "text" : "password",
                placeholder: unref(t)("desktop.plugin.aiChatbox.apiKey"),
                class: normalizeClass(["w-full bg-[var(--bg-card)] border rounded-md px-3 py-2 pr-10 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors", errors.apiKey ? "border-[var(--color-danger)]" : "border-[var(--border)]"])
              }, null, 10, _hoisted_13$2), [
                [vModelDynamic, form.apiKey]
              ]),
              createElementVNode("button", {
                class: "absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors",
                onClick: _cache[3] || (_cache[3] = ($event) => showApiKey.value = !showApiKey.value)
              }, [
                showApiKey.value ? (openBlock(), createElementBlock("svg", _hoisted_14$2, [..._cache[7] || (_cache[7] = [
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
                  }, null, -1)
                ])])) : (openBlock(), createElementBlock("svg", _hoisted_15$2, [..._cache[8] || (_cache[8] = [
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                  }, null, -1),
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                  }, null, -1)
                ])]))
              ])
            ]),
            errors.apiKey ? (openBlock(), createElementBlock("p", _hoisted_16$1, toDisplayString(errors.apiKey), 1)) : createCommentVNode("", true)
          ]),
          createElementVNode("div", null, [
            createElementVNode("label", _hoisted_17, toDisplayString(unref(t)("desktop.plugin.aiChatbox.apiFormat")), 1),
            withDirectives(createElementVNode("select", {
              "onUpdate:modelValue": _cache[4] || (_cache[4] = ($event) => form.apiFormat = $event),
              class: "w-full bg-[var(--bg-card)] border border-[var(--border)] rounded-md px-3 py-2 text-sm text-[var(--text-primary)] outline-none focus:border-brand focus:ring-1 focus:ring-brand/30 transition-colors appearance-none cursor-pointer"
            }, [
              (openBlock(true), createElementBlock(Fragment, null, renderList(unref(apiFormatOptions), (opt) => {
                return openBlock(), createElementBlock("option", {
                  key: opt.value,
                  value: opt.value
                }, toDisplayString(opt.label), 9, _hoisted_18);
              }), 128))
            ], 512), [
              [vModelSelect, form.apiFormat]
            ])
          ]),
          createVNode(_sfc_main$4, {
            models: form.models,
            onUpdate: _cache[5] || (_cache[5] = ($event) => form.models = $event)
          }, null, 8, ["models"]),
          errors.models ? (openBlock(), createElementBlock("p", _hoisted_19, toDisplayString(errors.models), 1)) : createCommentVNode("", true)
        ]),
        createElementVNode("div", _hoisted_20, [
          createElementVNode("button", {
            class: "px-4 py-2 text-sm bg-brand hover:bg-brand-hover text-white rounded-md transition-colors",
            onClick: handleSave
          }, toDisplayString(__props.mode === "add" ? unref(t)("desktop.plugin.aiChatbox.addProvider") : unref(t)("desktop.plugin.aiChatbox.saveProvider")), 1),
          __props.mode === "edit" ? (openBlock(), createElementBlock("button", {
            key: 0,
            class: "px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--color-danger)] hover:bg-[var(--bg-hover)]/80 rounded-md transition-colors",
            onClick: _cache[6] || (_cache[6] = ($event) => emit("delete", editingId.value))
          }, toDisplayString(unref(t)("desktop.plugin.aiChatbox.deleteProvider")), 1)) : createCommentVNode("", true)
        ])
      ]);
    };
  }
});
const _hoisted_1$2 = { class: "h-full flex flex-col bg-[var(--bg-page)]" };
const _hoisted_2$2 = { class: "h-12 flex items-center justify-between px-4 border-b border-[var(--border)] bg-[var(--bg-hover)]" };
const _hoisted_3$2 = { class: "text-sm font-medium text-[var(--text-primary)]" };
const _hoisted_4$2 = { class: "flex-1 flex overflow-hidden" };
const _hoisted_5$2 = { class: "flex-1 overflow-y-auto p-6" };
const _sfc_main$2 = /* @__PURE__ */ defineComponent({
  __name: "ProviderConfigPage",
  props: {
    providers: {}
  },
  emits: ["back", "add", "update", "remove"],
  setup(__props, { emit: __emit }) {
    const { t } = useI18n();
    const i18n = getI18n();
    const props = __props;
    const emit = __emit;
    const selectedProviderId = ref("");
    const selectedPresetName = ref("");
    const editingMode = ref("add");
    const formKey = ref(0);
    const formInitialValues = computed(() => {
      if (editingMode.value === "edit" && selectedProviderId.value) {
        return props.providers.find((p) => p.id === selectedProviderId.value);
      }
      return void 0;
    });
    const existingNames = computed(
      () => props.providers.map((p) => p.name)
    );
    function handleSelectProvider(id) {
      selectedProviderId.value = id;
      selectedPresetName.value = "";
      editingMode.value = "edit";
      formKey.value++;
    }
    function handleSelectPreset(preset) {
      selectedProviderId.value = "";
      selectedPresetName.value = preset.name;
      editingMode.value = "add";
      formKey.value++;
    }
    function handleAddNew() {
      selectedProviderId.value = "";
      selectedPresetName.value = "";
      editingMode.value = "add";
      formKey.value++;
    }
    function handleSave(provider) {
      if (editingMode.value === "edit" && selectedProviderId.value) {
        emit("update", selectedProviderId.value, provider);
      } else {
        emit("add", provider);
        selectedProviderId.value = provider.id;
        selectedPresetName.value = "";
        editingMode.value = "edit";
        formKey.value++;
      }
    }
    function handleDelete(id) {
      if (!confirm(i18n.global.t("desktop.plugin.aiChatbox.confirmDelete"))) return;
      emit("remove", id);
      selectedProviderId.value = "";
      selectedPresetName.value = "";
      editingMode.value = "add";
      formKey.value++;
    }
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1$2, [
        createElementVNode("header", _hoisted_2$2, [
          createElementVNode("button", {
            class: "flex items-center gap-1.5 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors",
            onClick: _cache[0] || (_cache[0] = ($event) => emit("back"))
          }, [
            _cache[1] || (_cache[1] = createElementVNode("svg", {
              class: "w-4 h-4",
              fill: "none",
              stroke: "currentColor",
              viewBox: "0 0 24 24"
            }, [
              createElementVNode("path", {
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                "stroke-width": "2",
                d: "M15 19l-7-7 7-7"
              })
            ], -1)),
            createTextVNode(" " + toDisplayString(unref(t)("desktop.plugin.aiChatbox.backToChat")), 1)
          ]),
          createElementVNode("h3", _hoisted_3$2, toDisplayString(unref(t)("desktop.plugin.aiChatbox.providerConfig")), 1),
          _cache[2] || (_cache[2] = createElementVNode("div", { class: "w-20" }, null, -1))
        ]),
        createElementVNode("div", _hoisted_4$2, [
          createVNode(_sfc_main$5, {
            providers: __props.providers,
            "selected-provider-id": selectedProviderId.value,
            "selected-preset-name": selectedPresetName.value,
            "is-add-mode": editingMode.value === "add" && !selectedPresetName.value,
            onSelectProvider: handleSelectProvider,
            onSelectPreset: handleSelectPreset,
            onAddNew: handleAddNew
          }, null, 8, ["providers", "selected-provider-id", "selected-preset-name", "is-add-mode"]),
          createElementVNode("div", _hoisted_5$2, [
            (openBlock(), createBlock(_sfc_main$3, {
              key: formKey.value,
              mode: editingMode.value,
              "initial-values": formInitialValues.value,
              "existing-names": existingNames.value,
              onSave: handleSave,
              onDelete: handleDelete
            }, null, 8, ["mode", "initial-values", "existing-names"]))
          ])
        ])
      ]);
    };
  }
});
const _hoisted_1$1 = {
  key: 0,
  class: "fixed inset-0 z-50 flex items-center justify-center p-4"
};
const _hoisted_2$1 = { class: "relative bg-[var(--bg-card)] rounded-xl shadow-2xl border border-[var(--border)] w-full max-w-lg" };
const _hoisted_3$1 = { class: "px-5 py-3 border-b border-[var(--border)]" };
const _hoisted_4$1 = { class: "text-base font-semibold text-[var(--text-primary)]" };
const _hoisted_5$1 = { class: "p-5 space-y-4" };
const _hoisted_6$1 = {
  key: 0,
  class: "p-3 bg-[var(--color-danger-light)] text-[var(--color-danger)] text-sm rounded-lg"
};
const _hoisted_7$1 = {
  key: 1,
  class: "flex items-center justify-center py-8"
};
const _hoisted_8$1 = { class: "flex items-center gap-2 text-[var(--text-secondary)]" };
const _hoisted_9$1 = { class: "block text-xs font-medium text-[var(--text-secondary)] mb-1" };
const _hoisted_10$1 = { class: "p-3 bg-[var(--bg-hover)] rounded-lg text-sm text-[var(--text-secondary)] whitespace-pre-wrap" };
const _hoisted_11$1 = { class: "block text-xs font-medium text-[var(--text-secondary)] mb-1" };
const _hoisted_12$1 = { class: "p-3 bg-brand-light rounded-lg text-sm text-[var(--text-brand)] whitespace-pre-wrap border border-brand/20" };
const _hoisted_13$1 = {
  key: 0,
  class: "px-5 py-3 border-t border-[var(--border)] flex justify-end gap-2"
};
const _hoisted_14$1 = ["disabled"];
const _hoisted_15$1 = {
  key: 1,
  class: "px-5 py-3 border-t border-[var(--border)] flex justify-end"
};
const _sfc_main$1 = /* @__PURE__ */ defineComponent({
  __name: "PromptOptimizeDialog",
  props: {
    show: { type: Boolean },
    optimizing: { type: Boolean },
    original: {},
    optimized: {},
    error: {}
  },
  emits: ["accept", "cancel"],
  setup(__props, { emit: __emit }) {
    const { t } = useI18n();
    const emit = __emit;
    return (_ctx, _cache) => {
      return __props.show ? (openBlock(), createElementBlock("div", _hoisted_1$1, [
        createElementVNode("div", {
          class: "absolute inset-0 bg-black/40 backdrop-blur-sm",
          onClick: _cache[0] || (_cache[0] = ($event) => emit("cancel"))
        }),
        createElementVNode("div", _hoisted_2$1, [
          createElementVNode("div", _hoisted_3$1, [
            createElementVNode("h3", _hoisted_4$1, toDisplayString(unref(t)("desktop.plugin.aiChatbox.optimizeTitle")), 1)
          ]),
          createElementVNode("div", _hoisted_5$1, [
            __props.error ? (openBlock(), createElementBlock("div", _hoisted_6$1, toDisplayString(__props.error), 1)) : __props.optimizing ? (openBlock(), createElementBlock("div", _hoisted_7$1, [
              createElementVNode("div", _hoisted_8$1, [
                _cache[4] || (_cache[4] = createElementVNode("svg", {
                  class: "w-5 h-5 animate-spin",
                  fill: "none",
                  viewBox: "0 0 24 24"
                }, [
                  createElementVNode("circle", {
                    class: "opacity-25",
                    cx: "12",
                    cy: "12",
                    r: "10",
                    stroke: "currentColor",
                    "stroke-width": "4"
                  }),
                  createElementVNode("path", {
                    class: "opacity-75",
                    fill: "currentColor",
                    d: "M4 12a8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.969 7.969 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                  })
                ], -1)),
                createTextVNode(" " + toDisplayString(unref(t)("desktop.plugin.aiChatbox.optimizing")), 1)
              ])
            ])) : (openBlock(), createElementBlock(Fragment, { key: 2 }, [
              createElementVNode("div", null, [
                createElementVNode("label", _hoisted_9$1, toDisplayString(unref(t)("desktop.plugin.aiChatbox.originalLabel")), 1),
                createElementVNode("div", _hoisted_10$1, toDisplayString(__props.original), 1)
              ]),
              createElementVNode("div", null, [
                createElementVNode("label", _hoisted_11$1, toDisplayString(unref(t)("desktop.plugin.aiChatbox.optimizedLabel")), 1),
                createElementVNode("div", _hoisted_12$1, toDisplayString(__props.optimized), 1)
              ])
            ], 64))
          ]),
          !__props.optimizing && !__props.error ? (openBlock(), createElementBlock("div", _hoisted_13$1, [
            createElementVNode("button", {
              class: "px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--text-primary)] rounded-btn hover:bg-[var(--bg-hover)]/80 transition-colors",
              onClick: _cache[1] || (_cache[1] = ($event) => emit("cancel"))
            }, toDisplayString(unref(t)("desktop.plugin.aiChatbox.cancel")), 1),
            createElementVNode("button", {
              disabled: !__props.optimized,
              class: "px-4 py-2 text-sm bg-brand hover:bg-brand-hover disabled:opacity-50 text-white rounded-btn transition-colors",
              onClick: _cache[2] || (_cache[2] = ($event) => emit("accept"))
            }, toDisplayString(unref(t)("desktop.plugin.aiChatbox.acceptAndFill")), 9, _hoisted_14$1)
          ])) : __props.error ? (openBlock(), createElementBlock("div", _hoisted_15$1, [
            createElementVNode("button", {
              class: "px-4 py-2 text-sm bg-[var(--bg-hover)] text-[var(--text-primary)] rounded-btn hover:bg-[var(--bg-hover)]/80 transition-colors",
              onClick: _cache[3] || (_cache[3] = ($event) => emit("cancel"))
            }, toDisplayString(unref(t)("desktop.plugin.aiChatbox.close")), 1)
          ])) : createCommentVNode("", true)
        ])
      ])) : createCommentVNode("", true);
    };
  }
});
function useAiConfig(storageGet, storageSet) {
  const providers = ref([]);
  const activeProviderId = ref("");
  const activeModel = ref("");
  const loading = ref(false);
  const showProviderManager = ref(false);
  const activeProvider = computed(
    () => providers.value.find((p) => p.id === activeProviderId.value)
  );
  const hasProvider = computed(() => providers.value.length > 0);
  async function loadConfig() {
    loading.value = true;
    try {
      const savedProviders = await storageGet("apiProviders");
      if (savedProviders) {
        const parsed = typeof savedProviders === "string" ? JSON.parse(savedProviders) : savedProviders;
        const rawList = Array.isArray(parsed) ? parsed : [];
        providers.value = rawList.map((p) => {
          if (!p.id) {
            return {
              id: generateId(),
              name: p.name || "",
              apiKey: p.apiKey || "",
              baseUrl: p.baseUrl || "",
              apiFormat: p.apiFormat || "openai",
              models: p.models || (p.model ? [p.model] : []),
              activeModel: p.activeModel || p.model || ""
            };
          }
          return p;
        });
        if (rawList.some((p) => !p.id)) {
          await saveConfig();
        }
      }
      const savedActiveId = await storageGet("activeProvider");
      activeProviderId.value = typeof savedActiveId === "string" ? savedActiveId : "";
      const savedActiveModel = await storageGet("activeModel");
      activeModel.value = typeof savedActiveModel === "string" ? savedActiveModel : "";
      if (!activeProviderId.value && providers.value.length > 0) {
        activeProviderId.value = providers.value[0].id;
      }
      const current = activeProvider.value;
      if (current && current.models.length > 0 && !current.models.includes(activeModel.value)) {
        activeModel.value = current.activeModel || current.models[0];
      }
    } catch (e) {
      console.error("[AI Chatbox] Failed to load config:", e);
    } finally {
      loading.value = false;
    }
  }
  async function saveConfig() {
    try {
      await storageSet("apiProviders", JSON.stringify(providers.value));
      await storageSet("activeProvider", activeProviderId.value);
      await storageSet("activeModel", activeModel.value);
    } catch (e) {
      console.error("[AI Chatbox] Failed to save config:", e);
    }
  }
  async function addProvider(provider) {
    if (providers.value.some((p) => p.name === provider.name)) {
      throw new Error("desktop.plugin.aiChatbox.providerExists");
    }
    providers.value.push(provider);
    if (!activeProviderId.value) {
      activeProviderId.value = provider.id;
      activeModel.value = provider.activeModel || provider.models[0] || "";
    }
    await saveConfig();
  }
  async function removeProvider(id) {
    var _a;
    providers.value = providers.value.filter((p) => p.id !== id);
    if (activeProviderId.value === id) {
      activeProviderId.value = ((_a = providers.value[0]) == null ? void 0 : _a.id) || "";
      const current = activeProvider.value;
      activeModel.value = (current == null ? void 0 : current.activeModel) || (current == null ? void 0 : current.models[0]) || "";
    }
    await saveConfig();
  }
  async function updateProvider(id, provider) {
    const index = providers.value.findIndex((p) => p.id === id);
    if (index === -1) return;
    providers.value[index] = provider;
    if (activeProviderId.value === id) {
      activeModel.value = provider.activeModel || provider.models[0] || "";
    }
    await saveConfig();
  }
  async function setActiveProvider(id) {
    if (!providers.value.some((p) => p.id === id)) return;
    activeProviderId.value = id;
    const current = providers.value.find((p) => p.id === id);
    activeModel.value = (current == null ? void 0 : current.activeModel) || (current == null ? void 0 : current.models[0]) || "";
    await saveConfig();
  }
  async function setActiveModel(modelId) {
    activeModel.value = modelId;
    const current = activeProvider.value;
    if (current) {
      current.activeModel = modelId;
    }
    await saveConfig();
  }
  async function addFromPreset(preset, apiKey) {
    const provider = {
      id: generateId(),
      name: preset.name,
      apiKey,
      baseUrl: preset.baseUrl,
      apiFormat: preset.apiFormat,
      models: [...preset.models],
      activeModel: preset.models[0] || ""
    };
    await addProvider(provider);
  }
  return {
    providers,
    activeProviderId,
    activeProvider,
    activeModel,
    hasProvider,
    loading,
    showProviderManager,
    loadConfig,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    setActiveModel,
    addFromPreset,
    PROVIDER_PRESETS
  };
}
function useAiChat(context) {
  const conversations = ref([]);
  const currentConvId = ref("");
  const messages = ref([]);
  const sending = ref(false);
  const streamingContent = ref("");
  const loadingHistory = ref(false);
  const currentConversation = computed(
    () => conversations.value.find((c) => c.id === currentConvId.value)
  );
  const isStreaming = computed(() => streamingContent.value !== "");
  function generateId2() {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  }
  async function loadConversations() {
    loadingHistory.value = true;
    try {
      const result = await context.commands.execute("ai-chatbox.list-conversations", {});
      if (result && Array.isArray(result)) {
        conversations.value = result;
      }
    } catch (e) {
      console.error("[AI Chatbox] Failed to load conversations:", e);
    } finally {
      loadingHistory.value = false;
    }
  }
  async function loadMessages(convId) {
    try {
      const result = await context.commands.execute("ai-chatbox.get-messages", { conversationId: convId });
      if (result && Array.isArray(result)) {
        messages.value = result;
      } else {
        messages.value = [];
      }
    } catch (e) {
      console.error("[AI Chatbox] Failed to load messages:", e);
      messages.value = [];
    }
    currentConvId.value = convId;
  }
  async function saveConversation(conv) {
    try {
      await context.commands.execute("ai-chatbox.save-conversation", { conversation: conv });
    } catch (e) {
      console.error("[AI Chatbox] Failed to save conversation:", e);
    }
  }
  async function saveMessage(conversationId, role, content, timestamp) {
    try {
      await context.commands.execute("ai-chatbox.save-message", { conversationId, role, content, timestamp });
    } catch (e) {
      console.error("[AI Chatbox] Failed to save message:", e);
    }
  }
  async function newConversation(providerName) {
    const conv = {
      id: generateId2(),
      title: "desktop.plugin.aiChatbox.newConversation",
      createdAt: (/* @__PURE__ */ new Date()).toISOString(),
      updatedAt: (/* @__PURE__ */ new Date()).toISOString(),
      providerName
    };
    conversations.value.unshift(conv);
    await saveConversation(conv);
    await loadMessages(conv.id);
  }
  async function deleteConversation(convId) {
    try {
      await context.commands.execute("ai-chatbox.delete-conversation", { conversationId: convId });
    } catch (e) {
      console.error("[AI Chatbox] Failed to delete conversation:", e);
    }
    conversations.value = conversations.value.filter((c) => c.id !== convId);
    if (currentConvId.value === convId) {
      currentConvId.value = "";
      messages.value = [];
    }
  }
  async function sendMessage(content) {
    const providersStr = await context.storage.get("apiProviders");
    const activeId = await context.storage.get("activeProvider");
    const currentModel = await context.storage.get("activeModel");
    let provider;
    if (providersStr && activeId) {
      try {
        const parsed = typeof providersStr === "string" ? JSON.parse(providersStr) : providersStr;
        const list = Array.isArray(parsed) ? parsed : [];
        provider = list.find((p) => p.id === activeId);
      } catch {
      }
    }
    if (!provider) throw new Error("desktop.plugin.aiChatbox.pleaseConfigure");
    const providerWithModel = { ...provider, model: currentModel || provider.activeModel || provider.models[0] || "" };
    if (!currentConvId.value) {
      await newConversation(provider.name);
    }
    const userMsg = {
      role: "user",
      content,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    messages.value.push(userMsg);
    await saveMessage(currentConvId.value, "user", content, userMsg.timestamp);
    const conv = conversations.value.find((c) => c.id === currentConvId.value);
    if (conv && conv.title === "desktop.plugin.aiChatbox.newConversation") {
      conv.title = content.slice(0, 30) + (content.length > 30 ? "..." : "");
      conv.updatedAt = (/* @__PURE__ */ new Date()).toISOString();
      await saveConversation(conv);
    }
    sending.value = true;
    streamingContent.value = "";
    const assistantMsg = {
      role: "assistant",
      content: "",
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    messages.value.push(assistantMsg);
    const requestMessages = messages.value.filter((m) => m.content || m.role === "assistant").slice(0, -1).map((m) => ({ role: m.role, content: m.content }));
    const streamId = generateId2();
    const streamDisposable = context.events.on(`ai-chatbox:stream:${streamId}`, (payload) => {
      if (payload.chunk) {
        streamingContent.value += payload.chunk;
        const last = messages.value[messages.value.length - 1];
        if (last && last.role === "assistant") {
          last.content = streamingContent.value;
        }
      } else if (payload.done) {
        streamDisposable.dispose();
        sending.value = false;
        streamingContent.value = "";
        const finalMsg = messages.value[messages.value.length - 1];
        if (finalMsg && finalMsg.role === "assistant") {
          saveMessage(currentConvId.value, "assistant", finalMsg.content, finalMsg.timestamp);
        }
        if (conv) {
          conv.updatedAt = (/* @__PURE__ */ new Date()).toISOString();
          saveConversation(conv);
        }
      } else if (payload.error) {
        streamDisposable.dispose();
        sending.value = false;
        streamingContent.value = "";
        const last = messages.value[messages.value.length - 1];
        if (last && last.role === "assistant") {
          last.content = `❌ ${payload.error}`;
        }
        saveMessage(currentConvId.value, "assistant", (last == null ? void 0 : last.content) || payload.error, assistantMsg.timestamp);
      }
    });
    try {
      await context.commands.execute("ai-chatbox.chat-stream", {
        streamId,
        provider: providerWithModel,
        messages: requestMessages
      });
    } catch (e) {
      streamDisposable.dispose();
      sending.value = false;
      streamingContent.value = "";
      const last = messages.value[messages.value.length - 1];
      if (last && last.role === "assistant") {
        last.content = `❌ ${e.message || "desktop.plugin.aiChatbox.requestFailed"}`;
      }
    }
  }
  function stopGeneration() {
    sending.value = false;
    streamingContent.value = "";
  }
  async function switchConversation(convId) {
    if (convId === currentConvId.value) return;
    await loadMessages(convId);
  }
  return {
    conversations,
    currentConvId,
    messages,
    sending,
    isStreaming,
    loadingHistory,
    currentConversation,
    loadConversations,
    newConversation,
    deleteConversation,
    sendMessage,
    stopGeneration,
    switchConversation
  };
}
function usePromptOptimizer(context) {
  const optimizing = ref(false);
  const showDialog = ref(false);
  const originalText = ref("");
  const optimizedText = ref("");
  const errorMessage = ref("");
  let currentSessionId = "";
  context.events.on("ai-chatbox:triggerOptimize", () => {
    optimizePrompt();
  });
  async function optimizePrompt() {
    const providersStr = await context.storage.get("apiProviders");
    const activeName = await context.storage.get("activeProvider");
    let provider;
    if (providersStr && activeName) {
      try {
        const parsed = typeof providersStr === "string" ? JSON.parse(providersStr) : providersStr;
        const list = Array.isArray(parsed) ? parsed : [];
        provider = list.find((p) => p.name === activeName);
      } catch {
      }
    }
    if (!provider) {
      errorMessage.value = "desktop.plugin.aiChatbox.pleaseConfigure";
      showDialog.value = true;
      return;
    }
    let sessionId = "";
    try {
      const sessions = await context.session.list();
      const activeSession = sessions.find((s) => s.status === "running");
      if (activeSession) {
        sessionId = activeSession.id;
      }
    } catch {
    }
    if (!sessionId) {
      errorMessage.value = "desktop.plugin.aiChatbox.noActiveSession";
      showDialog.value = true;
      return;
    }
    currentSessionId = sessionId;
    originalText.value = "";
    errorMessage.value = "";
    optimizing.value = true;
    showDialog.value = true;
    optimizedText.value = "";
    try {
      const result = await context.commands.execute("ai-chatbox.optimize-prompt", {
        provider,
        prompt: originalText.value || "请优化以下终端输入"
      });
      optimizedText.value = result;
    } catch (e) {
      errorMessage.value = e.message || "desktop.plugin.aiChatbox.optimizeFailed";
    } finally {
      optimizing.value = false;
    }
  }
  async function acceptOptimized() {
    if (!currentSessionId || !optimizedText.value) return;
    await context.terminal.sendInput(currentSessionId, "" + optimizedText.value);
    showDialog.value = false;
  }
  function cancelOptimize() {
    showDialog.value = false;
    originalText.value = "";
    optimizedText.value = "";
    errorMessage.value = "";
  }
  return {
    optimizing,
    showDialog,
    originalText,
    optimizedText,
    errorMessage,
    optimizePrompt,
    acceptOptimized,
    cancelOptimize
  };
}
const _hoisted_1 = { class: "h-full flex flex-col bg-[var(--bg-page)]" };
const _hoisted_2 = { class: "px-4 py-2 flex items-center justify-between border-b border-[var(--border)] bg-[var(--bg-hover)]" };
const _hoisted_3 = { class: "flex items-center gap-2" };
const _hoisted_4 = ["value"];
const _hoisted_5 = ["value"];
const _hoisted_6 = {
  key: 1,
  class: "text-xs text-[var(--text-tertiary)]"
};
const _hoisted_7 = ["value"];
const _hoisted_8 = ["value"];
const _hoisted_9 = { class: "flex items-center gap-1" };
const _hoisted_10 = ["title"];
const _hoisted_11 = ["disabled", "title"];
const _hoisted_12 = {
  key: 0,
  class: "flex-1 flex flex-col items-center justify-center p-6 text-center"
};
const _hoisted_13 = { class: "text-sm text-[var(--text-secondary)] mb-3" };
const _hoisted_14 = {
  key: 0,
  class: "flex flex-col items-center justify-center h-full text-center"
};
const _hoisted_15 = { class: "text-sm text-[var(--text-tertiary)]" };
const _hoisted_16 = { class: "border-t border-[var(--border)] p-3" };
const _sfc_main = /* @__PURE__ */ defineComponent({
  __name: "ChatView",
  setup(__props) {
    const { t } = useI18n();
    const context = inject("pluginContext");
    const {
      providers,
      activeProviderId,
      activeProvider,
      activeModel,
      hasProvider,
      loadConfig,
      setActiveProvider,
      setActiveModel,
      addProvider,
      updateProvider,
      removeProvider
    } = useAiConfig(context.storage.get, context.storage.set);
    const {
      messages,
      sending,
      isStreaming,
      loadConversations,
      newConversation,
      sendMessage
    } = useAiChat(context);
    const {
      showDialog: optimizeShowDialog,
      optimizing,
      originalText: optimizeOriginal,
      optimizedText: optimizeOptimized,
      errorMessage: optimizeError,
      acceptOptimized,
      cancelOptimize
    } = usePromptOptimizer(context);
    const messagesContainer = ref(null);
    const showConfigPage = ref(false);
    const currentModels = computed(() => {
      var _a;
      return ((_a = activeProvider.value) == null ? void 0 : _a.models) || [];
    });
    watch(() => messages.value.length, () => {
      nextTick(() => {
        if (messagesContainer.value) {
          messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight;
        }
      });
    });
    onMounted(async () => {
      await loadConfig();
      await loadConversations();
    });
    return (_ctx, _cache) => {
      return openBlock(), createElementBlock("div", _hoisted_1, [
        showConfigPage.value ? (openBlock(), createBlock(_sfc_main$2, {
          key: 0,
          providers: unref(providers),
          onBack: _cache[0] || (_cache[0] = ($event) => showConfigPage.value = false),
          onAdd: unref(addProvider),
          onUpdate: unref(updateProvider),
          onRemove: unref(removeProvider)
        }, null, 8, ["providers", "onAdd", "onUpdate", "onRemove"])) : (openBlock(), createElementBlock(Fragment, { key: 1 }, [
          createElementVNode("header", _hoisted_2, [
            createElementVNode("div", _hoisted_3, [
              unref(hasProvider) ? (openBlock(), createElementBlock("select", {
                key: 0,
                value: unref(activeProviderId),
                class: "bg-[var(--bg-card)] border border-[var(--border)] rounded px-2 py-1 text-xs text-[var(--text-primary)] outline-none",
                onChange: _cache[1] || (_cache[1] = ($event) => unref(setActiveProvider)($event.target.value))
              }, [
                (openBlock(true), createElementBlock(Fragment, null, renderList(unref(providers), (p) => {
                  return openBlock(), createElementBlock("option", {
                    key: p.id,
                    value: p.id
                  }, toDisplayString(p.name), 9, _hoisted_5);
                }), 128))
              ], 40, _hoisted_4)) : (openBlock(), createElementBlock("span", _hoisted_6, toDisplayString(unref(t)("desktop.plugin.aiChatbox.noProvider")), 1)),
              unref(hasProvider) && currentModels.value.length > 1 ? (openBlock(), createElementBlock("select", {
                key: 2,
                value: unref(activeModel),
                class: "bg-[var(--bg-card)] border border-[var(--border)] rounded px-2 py-1 text-xs text-[var(--text-primary)] outline-none",
                onChange: _cache[2] || (_cache[2] = ($event) => unref(setActiveModel)($event.target.value))
              }, [
                (openBlock(true), createElementBlock(Fragment, null, renderList(currentModels.value, (m) => {
                  return openBlock(), createElementBlock("option", {
                    key: m,
                    value: m
                  }, toDisplayString(m), 9, _hoisted_8);
                }), 128))
              ], 40, _hoisted_7)) : createCommentVNode("", true)
            ]),
            createElementVNode("div", _hoisted_9, [
              createElementVNode("button", {
                class: "p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors",
                title: unref(t)("desktop.plugin.aiChatbox.modelConfig"),
                onClick: _cache[3] || (_cache[3] = ($event) => showConfigPage.value = true)
              }, [..._cache[8] || (_cache[8] = [
                createElementVNode("svg", {
                  class: "w-4 h-4",
                  fill: "none",
                  stroke: "currentColor",
                  viewBox: "0 0 24 24"
                }, [
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
                  }),
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                  })
                ], -1)
              ])], 8, _hoisted_10),
              createElementVNode("button", {
                disabled: !unref(hasProvider),
                class: "p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded transition-colors disabled:opacity-50",
                title: unref(t)("desktop.plugin.aiChatbox.newConversation"),
                onClick: _cache[4] || (_cache[4] = ($event) => {
                  var _a;
                  return unref(newConversation)(((_a = unref(activeProvider)) == null ? void 0 : _a.name) || "");
                })
              }, [..._cache[9] || (_cache[9] = [
                createElementVNode("svg", {
                  class: "w-4 h-4",
                  fill: "none",
                  stroke: "currentColor",
                  viewBox: "0 0 24 24"
                }, [
                  createElementVNode("path", {
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "2",
                    d: "M12 4v16m8-8H4"
                  })
                ], -1)
              ])], 8, _hoisted_11)
            ])
          ]),
          !unref(hasProvider) ? (openBlock(), createElementBlock("div", _hoisted_12, [
            _cache[10] || (_cache[10] = createElementVNode("div", { class: "text-4xl mb-3" }, "🤖", -1)),
            createElementVNode("p", _hoisted_13, toDisplayString(unref(t)("desktop.plugin.aiChatbox.pleaseConfigure")), 1),
            createElementVNode("button", {
              class: "px-4 py-2 text-sm bg-brand hover:bg-brand-hover text-white rounded-btn transition-colors",
              onClick: _cache[5] || (_cache[5] = ($event) => showConfigPage.value = true)
            }, toDisplayString(unref(t)("desktop.plugin.aiChatbox.configureModel")), 1)
          ])) : (openBlock(), createElementBlock(Fragment, { key: 1 }, [
            createElementVNode("div", {
              ref_key: "messagesContainer",
              ref: messagesContainer,
              class: "flex-1 overflow-y-auto p-4 space-y-3"
            }, [
              unref(messages).length === 0 ? (openBlock(), createElementBlock("div", _hoisted_14, [
                _cache[11] || (_cache[11] = createElementVNode("div", { class: "text-3xl mb-2" }, "💬", -1)),
                createElementVNode("p", _hoisted_15, toDisplayString(unref(t)("desktop.plugin.aiChatbox.startNewChat")), 1)
              ])) : createCommentVNode("", true),
              (openBlock(true), createElementBlock(Fragment, null, renderList(unref(messages), (msg, i) => {
                return openBlock(), createBlock(_sfc_main$7, {
                  key: i,
                  message: msg,
                  streaming: unref(isStreaming) && i === unref(messages).length - 1
                }, null, 8, ["message", "streaming"]);
              }), 128))
            ], 512),
            createElementVNode("div", _hoisted_16, [
              createVNode(_sfc_main$6, {
                disabled: unref(sending) || !unref(activeProvider),
                placeholder: unref(t)("desktop.plugin.aiChatbox.inputPlaceholder"),
                onSend: unref(sendMessage)
              }, null, 8, ["disabled", "placeholder", "onSend"])
            ])
          ], 64)),
          createVNode(_sfc_main$1, {
            show: unref(optimizeShowDialog),
            optimizing: unref(optimizing),
            original: unref(optimizeOriginal),
            optimized: unref(optimizeOptimized),
            error: unref(optimizeError),
            onAccept: _cache[6] || (_cache[6] = ($event) => unref(acceptOptimized)()),
            onCancel: _cache[7] || (_cache[7] = ($event) => unref(cancelOptimize)())
          }, null, 8, ["show", "optimizing", "original", "optimized", "error"])
        ], 64))
      ]);
    };
  }
});
async function activate(context) {
  context.ui.registerSidebarPanel({
    id: "ai-chatbox.sidebar",
    title: "AI 对话",
    component: _sfc_main
  });
  context.ui.registerTerminalToolbarItem({
    id: "ai-optimize-prompt",
    label: "AI 优化",
    icon: "✨",
    onClick: () => context.events.emit("ai-chatbox:triggerOptimize")
  });
  console.log("[AI Chatbox] Plugin activated (rust-ts mode)");
}
async function deactivate() {
  console.log("[AI Chatbox] Plugin deactivated");
}
export {
  activate,
  deactivate
};
