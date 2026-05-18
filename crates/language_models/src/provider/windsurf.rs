use anyhow::Result;
use collections::BTreeMap;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gpui::{AnyView, App, AsyncApp, Context, Entity, SharedString, Task, TaskExt, Window};
use http_client::{AsyncBody, HttpClient, Method, Request as HttpRequest};
use language_model::{
    ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelProviderState,
    LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolSchemaFormat, RateLimiter,
    env_var,
};
use menu;
use open_ai::{ResponseStreamEvent, stream_completion};
use settings::{Settings, SettingsStore};
use std::sync::{Arc, LazyLock};
use ui::{ButtonLink, ConfiguredApiCard, List, ListBulletItem, prelude::*};
use ui_input::InputField;
use util::ResultExt;

use crate::provider::open_ai::{OpenAiEventMapper, into_open_ai};

pub use settings::WindsurfAvailableModel as AvailableModel;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("windsurf");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("Windsurf");

const DEFAULT_API_URL: &str = "http://localhost:3003/v1";
const AUTH_TOKEN_ENV_VAR_NAME: &str = "WINDSURF_AUTH_TOKEN";
static AUTH_TOKEN_ENV_VAR: LazyLock<EnvVar> = env_var!(AUTH_TOKEN_ENV_VAR_NAME);

#[derive(Default, Clone, Debug, PartialEq)]
pub struct WindsurfSettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
}

pub struct WindsurfLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

pub struct State {
    auto_authenticated: bool,
    fetched_models: Option<Vec<AvailableModel>>,
    api_key_state: ApiKeyState,
    credentials_provider: Arc<dyn CredentialsProvider>,
    http_client: Arc<dyn HttpClient>,
}

impl State {
    fn is_authenticated(&self) -> bool {
        self.auto_authenticated || self.api_key_state.has_key()
    }

    fn set_api_key(&mut self, token: Option<String>, cx: &mut Context<Self>) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        let http_client = self.http_client.clone();
        let register_token = token.clone();
        let register_url = api_url.clone();
        let store_task = self.api_key_state.store(
            api_url,
            token,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );
        cx.spawn(async move |_, _| {
            store_task.await?;
            if let Some(t) = register_token {
                register_api_key_with_proxy(http_client.as_ref(), &register_url, &t)
                    .await
                    .log_err();
            }
            Ok(())
        })
    }

    fn authenticate(&mut self, cx: &mut Context<Self>) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        let http_client = self.http_client.clone();
        let load_task = self.api_key_state.load_if_needed(
            api_url.clone(),
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );
        cx.spawn(async move |this, cx| {
            load_task.await?;
            // Try auto-registration from local Windsurf installation first
            let auto_ok = auto_register_windsurf_tokens(http_client.as_ref(), &api_url).await;
            if auto_ok {
                this.update(cx, |this, cx| {
                    this.auto_authenticated = true;
                    cx.notify();
                })
                .log_err();
            } else {
                // Fall back to manually stored credential
                if let Ok(Some(token)) = this.read_with(cx, |this, _| this.api_key_state.key(&api_url)) {
                    register_api_key_with_proxy(http_client.as_ref(), &api_url, &token)
                        .await
                        .log_err();
                }
            }
            // Fetch model list from proxy regardless of auth method
            if let Ok(models) = fetch_models_from_proxy(http_client.as_ref(), &api_url).await {
                this.update(cx, |this, cx| {
                    this.fetched_models = Some(models);
                    cx.notify();
                })
                .log_err();
            }
            Ok(())
        })
    }
}

impl WindsurfLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|cx| {
            cx.observe_global::<SettingsStore>(|this: &mut State, cx| {
                let credentials_provider = this.credentials_provider.clone();
                let api_url = WindsurfLanguageModelProvider::api_url(cx);
                this.api_key_state.handle_url_change(
                    api_url,
                    |this| &mut this.api_key_state,
                    credentials_provider,
                    cx,
                );
                cx.notify();
            })
            .detach();
            State {
                auto_authenticated: false,
                fetched_models: None,
                api_key_state: ApiKeyState::new(Self::api_url(cx), (*AUTH_TOKEN_ENV_VAR).clone()),
                credentials_provider,
                http_client: http_client.clone(),
            }
        });

        Self { http_client, state }
    }

    fn create_language_model(&self, model: AvailableModel) -> Arc<dyn LanguageModel> {
        Arc::new(WindsurfLanguageModel {
            id: LanguageModelId::from(model.name.clone()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }

    fn settings(cx: &App) -> &WindsurfSettings {
        &crate::AllLanguageModelSettings::get_global(cx).windsurf
    }

    fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            DEFAULT_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }
}

impl LanguageModelProviderState for WindsurfLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for WindsurfLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiOpenAiCompat)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        default_models()
            .into_iter()
            .next()
            .map(|model| self.create_language_model(model))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        default_models()
            .into_iter()
            .find(|m| m.name.contains("haiku") || m.name.contains("flash"))
            .map(|model| self.create_language_model(model))
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models: BTreeMap<String, AvailableModel> = BTreeMap::default();

        let base = self.state.read(cx)
            .fetched_models
            .clone()
            .unwrap_or_else(default_models);
        for model in base {
            models.insert(model.name.clone(), model);
        }

        for model in &WindsurfLanguageModelProvider::settings(cx).available_models {
            models.insert(model.name.clone(), model.clone());
        }

        models
            .into_values()
            .map(|model| self.create_language_model(model))
            .collect()
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated()
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        self.state.update(cx, |state, cx| state.authenticate(cx))
    }

    fn configuration_view(
        &self,
        _target_agent: language_model::ConfigurationViewTargetAgent,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| ConfigurationView::new(self.state.clone(), window, cx))
            .into()
    }

    fn reset_credentials(&self, cx: &mut App) -> Task<Result<()>> {
        self.state
            .update(cx, |state, cx| state.set_api_key(None, cx))
    }
}

fn default_models() -> Vec<AvailableModel> {
    use settings::OpenAiCompatibleModelCapabilities as Cap;

    let tools_and_images = Cap {
        tools: true,
        images: true,
        parallel_tool_calls: false,
        prompt_cache_key: false,
        chat_completions: true,
        interleaved_reasoning: false,
    };
    let tools_only = Cap {
        tools: true,
        images: false,
        parallel_tool_calls: false,
        prompt_cache_key: false,
        chat_completions: true,
        interleaved_reasoning: false,
    };

    vec![
        AvailableModel {
            name: "claude-sonnet-4.6".into(),
            display_name: Some("Claude Sonnet 4.6 (Windsurf)".into()),
            max_tokens: 200_000,
            max_output_tokens: Some(8_192),
            capabilities: tools_and_images.clone(),
        },
        AvailableModel {
            name: "claude-opus-4.6".into(),
            display_name: Some("Claude Opus 4.6 (Windsurf)".into()),
            max_tokens: 200_000,
            max_output_tokens: Some(8_192),
            capabilities: tools_and_images.clone(),
        },
        AvailableModel {
            name: "claude-haiku-4.5".into(),
            display_name: Some("Claude Haiku 4.5 (Windsurf)".into()),
            max_tokens: 200_000,
            max_output_tokens: Some(8_192),
            capabilities: tools_and_images.clone(),
        },
        AvailableModel {
            name: "gpt-5".into(),
            display_name: Some("GPT-5 (Windsurf)".into()),
            max_tokens: 128_000,
            max_output_tokens: Some(16_384),
            capabilities: tools_and_images.clone(),
        },
        AvailableModel {
            name: "gemini-2.5-pro".into(),
            display_name: Some("Gemini 2.5 Pro (Windsurf)".into()),
            max_tokens: 1_048_576,
            max_output_tokens: Some(8_192),
            capabilities: tools_only.clone(),
        },
        AvailableModel {
            name: "gemini-2.5-flash".into(),
            display_name: Some("Gemini 2.5 Flash (Windsurf)".into()),
            max_tokens: 1_048_576,
            max_output_tokens: Some(8_192),
            capabilities: tools_only.clone(),
        },
        AvailableModel {
            name: "kimi-k2".into(),
            display_name: Some("Kimi K2 (Windsurf)".into()),
            max_tokens: 128_000,
            max_output_tokens: Some(8_192),
            capabilities: tools_only.clone(),
        },
        AvailableModel {
            name: "glm-4.7".into(),
            display_name: Some("GLM-4.7 (Windsurf)".into()),
            max_tokens: 128_000,
            max_output_tokens: Some(8_192),
            capabilities: tools_only,
        },
    ]
}

pub struct WindsurfLanguageModel {
    id: LanguageModelId,
    model: AvailableModel,
    state: Entity<State>,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl WindsurfLanguageModel {
    fn stream_completion(
        &self,
        request: open_ai::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<'static, Result<ResponseStreamEvent>>,
            LanguageModelCompletionError,
        >,
    > {
        let http_client = self.http_client.clone();

        let api_url = self.state.read_with(cx, |_state, cx| {
            WindsurfLanguageModelProvider::api_url(cx)
        });

        let provider = PROVIDER_NAME;
        let future = self.request_limiter.stream(async move {
            let response = stream_completion(
                http_client.as_ref(),
                provider.0.as_str(),
                &api_url,
                "",
                request,
            )
            .await?;
            Ok(response)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}

impl LanguageModel for WindsurfLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(
            self.model
                .display_name
                .clone()
                .unwrap_or_else(|| self.model.name.clone()),
        )
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        self.model.capabilities.tools
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        LanguageModelToolSchemaFormat::JsonSchemaSubset
    }

    fn supports_images(&self) -> bool {
        self.model.capabilities.images
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto => self.model.capabilities.tools,
            LanguageModelToolChoice::Any => self.model.capabilities.tools,
            LanguageModelToolChoice::None => true,
        }
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_split_token_display(&self) -> bool {
        true
    }

    fn telemetry_id(&self) -> String {
        format!("windsurf/{}", self.model.name)
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_tokens
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<
                'static,
                Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
            >,
            LanguageModelCompletionError,
        >,
    > {
        let request = into_open_ai(
            request,
            &self.model.name,
            self.model.capabilities.parallel_tool_calls,
            self.model.capabilities.prompt_cache_key,
            self.max_output_tokens(),
            None,
            false,
        );
        let completions = self.stream_completion(request, cx);
        async move {
            let mapper = OpenAiEventMapper::new();
            Ok(mapper.map_stream(completions.await?).boxed())
        }
        .boxed()
    }
}

struct ConfigurationView {
    api_key_editor: Entity<InputField>,
    state: Entity<State>,
    load_credentials_task: Option<Task<()>>,
}

impl ConfigurationView {
    fn new(state: Entity<State>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let api_key_editor = cx.new(|cx| {
            InputField::new(
                window,
                cx,
                "Your Windsurf auth token (from windsurf.com/show-auth-token)",
            )
        });

        cx.observe(&state, |_, _, cx| {
            cx.notify();
        })
        .detach();

        let load_credentials_task = Some(cx.spawn_in(window, {
            let state = state.clone();
            async move |this, cx| {
                if let Some(task) = Some(state.update(cx, |state, cx| state.authenticate(cx))) {
                    let _ = task.await;
                }
                this.update(cx, |this, cx| {
                    this.load_credentials_task = None;
                    cx.notify();
                })
                .log_err();
            }
        }));

        Self {
            api_key_editor,
            state,
            load_credentials_task,
        }
    }

    fn save_api_key(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let api_key = self.api_key_editor.read(cx).text(cx).trim().to_string();
        if api_key.is_empty() {
            return;
        }

        self.api_key_editor
            .update(cx, |input, cx| input.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(Some(api_key), cx))
                .await
        })
        .detach_and_log_err(cx);
    }

    fn reset_api_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.api_key_editor
            .update(cx, |input, cx| input.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(None, cx))
                .await
        })
        .detach_and_log_err(cx);
    }

    fn should_render_editor(&self, cx: &mut Context<Self>) -> bool {
        !self.state.read(cx).is_authenticated()
    }
}

impl Render for ConfigurationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let auto_authenticated = self.state.read(cx).auto_authenticated;
        let env_var_set = self.state.read(cx).api_key_state.is_from_env_var();
        let configured_card_label = if auto_authenticated {
            "Auto-configured from local Windsurf installation".to_string()
        } else if env_var_set {
            format!("Auth token set in {AUTH_TOKEN_ENV_VAR_NAME} environment variable")
        } else {
            let api_url = WindsurfLanguageModelProvider::api_url(cx);
            if api_url == DEFAULT_API_URL {
                "Auth token configured".to_string()
            } else {
                format!("Auth token configured for {api_url}")
            }
        };

        let setup_instructions = v_flex()
            .gap_2()
            .child(Label::new(
                "Windsurf accounts were not detected automatically. Start the local proxy and paste your token below.",
            ))
            .child(
                List::new()
                    .child(
                        ListBulletItem::new("")
                            .child(Label::new("Get your token at"))
                            .child(ButtonLink::new(
                                "windsurf.com/show-auth-token",
                                "https://windsurf.com/show-auth-token",
                            )),
                    )
                    .child(ListBulletItem::new(
                        "Paste it below and press Enter. Done.",
                    )),
            );

        let api_key_section = if self.should_render_editor(cx) {
            v_flex()
                .on_action(cx.listener(Self::save_api_key))
                .child(setup_instructions)
                .child(
                    div()
                        .pt(DynamicSpacing::Base04.rems(cx))
                        .child(self.api_key_editor.clone()),
                )
                .child(
                    Label::new(format!(
                        "You can also set the {AUTH_TOKEN_ENV_VAR_NAME} environment variable and restart Zed."
                    ))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                )
                .into_any_element()
        } else {
            ConfiguredApiCard::new(configured_card_label)
                .disabled(auto_authenticated || env_var_set)
                .when(auto_authenticated, |this| {
                    this.tooltip_label("Windsurf accounts auto-loaded from your local installation. Re-open Zed to refresh.")
                })
                .when(!auto_authenticated && env_var_set, |this| {
                    this.tooltip_label(format!(
                        "To reset your auth token, unset the {AUTH_TOKEN_ENV_VAR_NAME} environment variable."
                    ))
                })
                .when(!auto_authenticated && !env_var_set, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| this.reset_api_key(window, cx)))
                })
                .into_any_element()
        };

        if self.load_credentials_task.is_some() {
            div().child(Label::new("Loading credentials…")).into_any()
        } else {
            v_flex().size_full().child(api_key_section).into_any()
        }
    }
}

async fn register_api_key_with_proxy(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
    api_key: &str,
) -> Result<()> {
    let base_url = proxy_api_url
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let uri = format!("{base_url}/auth/login");
    let escaped = api_key.replace('"', "\\\"");
    let body = format!("{{\"api_key\":\"{escaped}\"}}");
    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(body))?;
    let mut response = http_client.send(request).await?;
    if !response.status().is_success() {
        use futures::AsyncReadExt;
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await.ok();
        anyhow::bail!("WindsurfAPI proxy login failed ({}): {}", response.status(), body);
    }
    Ok(())
}

async fn auto_register_windsurf_tokens(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
) -> bool {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return false,
    };
    let cache_path = format!(
        "{home}/Library/Application Support/Windsurf/User/globalStorage/\
sparkcore.xinghuo-windsurf/devin-session-cache.json"
    );
    let content = match std::fs::read_to_string(&cache_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let entries: Vec<&serde_json::Value> = match &parsed {
        serde_json::Value::Object(map) => map.values().collect(),
        serde_json::Value::Array(arr) => arr.iter().collect(),
        _ => return false,
    };
    let mut any_ok = false;
    for entry in entries {
        let token = match entry.get("token").and_then(|t| t.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => continue,
        };
        if register_api_key_with_proxy(http_client, proxy_api_url, &token)
            .await
            .is_ok()
        {
            any_ok = true;
        }
    }
    any_ok
}

async fn fetch_models_from_proxy(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
) -> Result<Vec<AvailableModel>> {
    use futures::AsyncReadExt;
    use settings::OpenAiCompatibleModelCapabilities as Cap;

    let uri = format!("{proxy_api_url}/models");
    let request = HttpRequest::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Authorization", "Bearer ")
        .body(AsyncBody::empty())?;
    let mut response = http_client.send(request).await?;
    if !response.status().is_success() {
        anyhow::bail!("models fetch failed: {}", response.status());
    }
    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let data = json["data"].as_array().ok_or_else(|| anyhow::anyhow!("no data field"))?;

    let tools_cap = Cap {
        tools: true,
        images: false,
        parallel_tool_calls: false,
        prompt_cache_key: false,
        chat_completions: true,
        interleaved_reasoning: false,
    };

    let models = data
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            Some(AvailableModel {
                name: id.to_string(),
                display_name: Some(format!("{id} (Windsurf)")),
                max_tokens: infer_context_size(id),
                max_output_tokens: Some(infer_output_tokens(id)),
                capabilities: tools_cap.clone(),
            })
        })
        .collect();
    Ok(models)
}

fn infer_context_size(model_id: &str) -> u64 {
    let id = model_id.to_lowercase();
    if id.contains("gemini") { 1_048_576 }
    else if id.contains("claude") { 200_000 }
    else if id.contains("gpt-5") || id.contains("o3") || id.contains("o4") { 200_000 }
    else if id.contains("gpt-4") { 128_000 }
    else if id.contains("deepseek") { 163_840 }
    else if id.contains("kimi") { 131_072 }
    else { 128_000 }
}

fn infer_output_tokens(model_id: &str) -> u64 {
    let id = model_id.to_lowercase();
    if id.contains("gemini-2.5-pro") { 65_536 }
    else if id.contains("gemini") { 8_192 }
    else if id.contains("claude") { 8_192 }
    else { 16_384 }
}

