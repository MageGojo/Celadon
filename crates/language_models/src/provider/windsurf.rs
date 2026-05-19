use anyhow::Result;
use collections::BTreeMap;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gpui::{AnyView, App, AsyncApp, ClickEvent, Context, Entity, SharedString, Task, Window};
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
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use ui::prelude::*;
use ui_input::InputField;
use util::ResultExt;

use crate::provider::open_ai::{OpenAiEventMapper, into_open_ai};

pub use settings::WindsurfAvailableModel as AvailableModel;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("windsurf");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("Windsurf");

const DEFAULT_API_URL: &str = "http://localhost:3003/v1";
const AUTH_TOKEN_ENV_VAR_NAME: &str = "WINDSURF_AUTH_TOKEN";
static AUTH_TOKEN_ENV_VAR: LazyLock<EnvVar> = env_var!(AUTH_TOKEN_ENV_VAR_NAME);

#[derive(Clone, Debug)]
struct PausedAccount {
    info: ProxyAccount,
    email: Option<String>,
    password: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct ProxyAccount {
    id: String,
    email: String,
    tier: String,
    status: String,
    daily_percent: Option<u32>,
    weekly_percent: Option<u32>,
    plan_name: Option<String>,
    last_used: Option<String>,
    ok_model_count: u32,
    error_count: u32,
}

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
    proxy_accounts: Vec<ProxyAccount>,
    accounts_loading: bool,
    add_account_error: Option<String>,
    selected_account_ids: HashSet<String>,
    exclusive_account_id: Option<String>,
    paused_accounts: Vec<PausedAccount>,
    known_credentials: HashMap<String, String>,
}

impl State {
    fn is_authenticated(&self) -> bool {
        self.auto_authenticated
            || self.api_key_state.has_key()
            || !self.proxy_accounts.is_empty()
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
        cx.spawn(async move |this, cx| {
            // Check proxy accounts FIRST — if any exist, never touch the macOS keychain.
            // This prevents the "zed wants to access keychain" dialog on every startup.
            let existing = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                .await
                .unwrap_or_default();
            let has_existing = !existing.is_empty();
            this.update(cx, |this, cx| {
                this.proxy_accounts = existing;
                cx.notify();
            })
            .log_err();

            if !has_existing {
                // No proxy accounts — fall back to keychain / env var / auto-register
                let load_task = this
                    .update(cx, |this, cx| {
                        this.api_key_state.load_if_needed(
                            api_url.clone(),
                            |s| &mut s.api_key_state,
                            credentials_provider,
                            cx,
                        )
                    })
                    .map_err(|_| AuthenticateError::CredentialsNotFound)?;
                load_task.await?;

                let auto_ok =
                    auto_register_windsurf_tokens(http_client.as_ref(), &api_url).await;
                if auto_ok {
                    this.update(cx, |this, cx| {
                        this.auto_authenticated = true;
                        cx.notify();
                    })
                    .log_err();
                } else if let Ok(Some(token)) =
                    this.read_with(cx, |this, _| this.api_key_state.key(&api_url))
                {
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
            // Fetch proxy account list (again after auth to get fresh credits)
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url).await;
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts.unwrap_or_default();
                cx.notify();
            })
            .log_err();

            // Start periodic background refresh every 5 minutes
            cx.spawn({
                let this = this.clone();
                async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(300))
                            .await;
                        let Ok(task) =
                            this.update(cx, |state, cx| state.refresh_accounts(cx))
                        else {
                            break;
                        };
                        task.await;
                    }
                }
            })
            .detach();

            Ok(())
        })
    }

    fn refresh_accounts(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        self.accounts_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url).await;
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts.unwrap_or_default();
                this.accounts_loading = false;
                cx.notify();
            })
            .log_err();
        })
    }

    fn add_email_account(
        &mut self,
        email: String,
        password: String,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        self.add_account_error = None;
        self.known_credentials.insert(email.clone(), password.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            match add_email_account_to_proxy(http_client.as_ref(), &api_url, &email, &password)
                .await
            {
                Ok(_) => {
                    let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                        .await
                        .unwrap_or_default();
                    this.update(cx, |this, cx| {
                        this.proxy_accounts = accounts;
                        this.auto_authenticated = !this.proxy_accounts.is_empty();
                        this.add_account_error = None;
                        cx.notify();
                    })
                    .log_err();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.add_account_error = Some(e.to_string());
                        cx.notify();
                    })
                    .log_err();
                }
            }
        })
    }

    fn remove_account(&mut self, account_id: String, cx: &mut Context<Self>) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        cx.spawn(async move |this, cx| {
            if remove_proxy_account(http_client.as_ref(), &api_url, &account_id)
                .await
                .is_ok()
            {
                let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                    .await
                    .unwrap_or_default();
                this.update(cx, |this, cx| {
                    this.proxy_accounts = accounts;
                    this.selected_account_ids.remove(&account_id);
                    if this.proxy_accounts.is_empty() {
                        this.auto_authenticated = false;
                    }
                    cx.notify();
                })
                .log_err();
            }
        })
    }

    fn toggle_selection(&mut self, account_id: String, cx: &mut Context<Self>) {
        if self.selected_account_ids.contains(&account_id) {
            self.selected_account_ids.remove(&account_id);
        } else {
            self.selected_account_ids.insert(account_id);
        }
        cx.notify();
    }

    fn select_all_accounts(&mut self, cx: &mut Context<Self>) {
        self.selected_account_ids = self.proxy_accounts.iter().map(|a| a.id.clone()).collect();
        cx.notify();
    }

    fn deselect_all_accounts(&mut self, cx: &mut Context<Self>) {
        self.selected_account_ids.clear();
        cx.notify();
    }

    fn set_exclusive_account(&mut self, account_id: String, cx: &mut Context<Self>) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        let to_pause: Vec<PausedAccount> = self
            .proxy_accounts
            .iter()
            .filter(|a| a.id != account_id)
            .map(|a| {
                let password = self.known_credentials.get(&a.email).cloned();
                PausedAccount {
                    info: a.clone(),
                    email: Some(a.email.clone()),
                    password,
                }
            })
            .collect();
        let ids_to_remove: Vec<String> = to_pause.iter().map(|p| p.info.id.clone()).collect();
        self.paused_accounts.extend(to_pause);
        self.exclusive_account_id = Some(account_id);
        cx.spawn(async move |this, cx| {
            for id in &ids_to_remove {
                remove_proxy_account(http_client.as_ref(), &api_url, id)
                    .await
                    .log_err();
            }
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                .await
                .unwrap_or_default();
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts;
                cx.notify();
            })
            .log_err();
        })
    }

    fn restore_all_accounts(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        let paused = std::mem::take(&mut self.paused_accounts);
        let already_in_pool: HashSet<String> = self
            .proxy_accounts
            .iter()
            .map(|a| a.email.clone())
            .collect();
        self.exclusive_account_id = None;
        cx.spawn(async move |this, cx| {
            for account in &paused {
                if let (Some(email), Some(password)) = (&account.email, &account.password) {
                    if already_in_pool.contains(email) {
                        continue;
                    }
                    add_email_account_to_proxy(http_client.as_ref(), &api_url, email, password)
                        .await
                        .log_err();
                }
            }
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                .await
                .unwrap_or_default();
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts;
                this.auto_authenticated = !this.proxy_accounts.is_empty();
                cx.notify();
            })
            .log_err();
        })
    }

    fn clear_exclusive_account(&mut self, cx: &mut Context<Self>) {
        self.exclusive_account_id = None;
        self.paused_accounts.clear();
        cx.notify();
    }

    fn remove_broken_accounts(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        let broken_ids: Vec<String> = self
            .proxy_accounts
            .iter()
            .filter(|a| a.ok_model_count == 0)
            .map(|a| a.id.clone())
            .collect();
        cx.spawn(async move |this, cx| {
            for id in &broken_ids {
                remove_proxy_account(http_client.as_ref(), &api_url, id)
                    .await
                    .log_err();
            }
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                .await
                .unwrap_or_default();
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts;
                if this.proxy_accounts.is_empty() {
                    this.auto_authenticated = false;
                }
                cx.notify();
            })
            .log_err();
        })
    }

    fn remove_selected_accounts(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let ids: Vec<String> = self.selected_account_ids.iter().cloned().collect();
        let http_client = self.http_client.clone();
        let api_url = WindsurfLanguageModelProvider::api_url(cx);
        self.selected_account_ids.clear();
        cx.spawn(async move |this, cx| {
            for id in &ids {
                remove_proxy_account(http_client.as_ref(), &api_url, id)
                    .await
                    .log_err();
            }
            let accounts = fetch_proxy_accounts(http_client.as_ref(), &api_url)
                .await
                .unwrap_or_default();
            this.update(cx, |this, cx| {
                this.proxy_accounts = accounts;
                if this.proxy_accounts.is_empty() {
                    this.auto_authenticated = false;
                }
                cx.notify();
            })
            .log_err();
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
                proxy_accounts: Vec::new(),
                accounts_loading: false,
                add_account_error: None,
                selected_account_ids: HashSet::new(),
                exclusive_account_id: None,
                paused_accounts: Vec::new(),
                known_credentials: HashMap::new(),
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
    email_editor: Entity<InputField>,
    password_editor: Entity<InputField>,
    state: Entity<State>,
    load_credentials_task: Option<Task<()>>,
}

impl ConfigurationView {
    fn new(state: Entity<State>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let email_editor = cx.new(|cx| InputField::new(window, cx, "Email"));
        let password_editor =
            cx.new(|cx| InputField::new(window, cx, "Password").masked(true));

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
            email_editor,
            password_editor,
            state,
            load_credentials_task,
        }
    }

    fn add_account(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.email_editor.read(cx).text(cx).trim().to_string();
        let password = self.password_editor.read(cx).text(cx).trim().to_string();
        if email.is_empty() || password.is_empty() {
            return;
        }
        self.email_editor
            .update(cx, |input, cx| input.set_text("", window, cx));
        self.password_editor
            .update(cx, |input, cx| input.set_text("", window, cx));
        self.state
            .update(cx, |state, cx| state.add_email_account(email, password, cx))
            .detach();
    }

    fn delete_account(&mut self, account_id: String, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.remove_account(account_id, cx))
            .detach();
    }

    fn refresh_accounts(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.refresh_accounts(cx))
            .detach();
    }
}

impl Render for ConfigurationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.load_credentials_task.is_some() {
            return div()
                .child(Label::new("Loading credentials…").color(Color::Muted))
                .into_any();
        }

        let env_var_set = self.state.read(cx).api_key_state.is_from_env_var();
        if env_var_set {
            return v_flex()
                .gap_2()
                .child(Label::new(format!(
                    "Auth token set via {AUTH_TOKEN_ENV_VAR_NAME} environment variable."
                )))
                .child(
                    Label::new("Unset the environment variable and restart Celadon to manage accounts here.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any();
        }

        let accounts = self.state.read(cx).proxy_accounts.clone();
        let accounts_loading = self.state.read(cx).accounts_loading;
        let add_error = self.state.read(cx).add_account_error.clone();

        let selected_ids = self.state.read(cx).selected_account_ids.clone();
        let selected_count = selected_ids.len();
        let all_selected = !accounts.is_empty() && selected_count == accounts.len();
        let exclusive_id = self.state.read(cx).exclusive_account_id.clone();
        let paused = self.state.read(cx).paused_accounts.clone();
        let has_broken = accounts.iter().any(|a| a.ok_model_count == 0);
        let restorable_count = paused.iter().filter(|p| p.password.is_some()).count();
        let paused_count = paused.len();

        let current_id = accounts
            .iter()
            .filter(|a| a.last_used.is_some())
            .max_by(|a, b| a.last_used.cmp(&b.last_used))
            .map(|a| a.id.clone());

        let paused_rows = paused.iter().map(|paused_acct| {
            let can_restore = paused_acct.password.is_some();
            let email = paused_acct.info.email.clone();
            let caps_label = if paused_acct.info.ok_model_count == 0 {
                SharedString::from("no models")
            } else {
                SharedString::from(format!("{} models", paused_acct.info.ok_model_count))
            };
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .py_1()
                .opacity(0.4)
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Label::new("⏸")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(Label::new(email).color(Color::Muted))
                        .child(
                            Label::new(caps_label)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .when(!can_restore, |this| {
                            this.child(
                                Label::new("⚠ no creds")
                                    .size(LabelSize::Small)
                                    .color(Color::Warning),
                            )
                        }),
                )
                .child(
                    Label::new("paused")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
        });

        let account_rows = accounts.iter().map(|account| {
            let account_id = account.id.clone();
            let is_selected = selected_ids.contains(&account_id);
            let tier_color = match account.tier.as_str() {
                "pro" => Color::Success,
                "free" => Color::Accent,
                _ => Color::Muted,
            };
            let status_color = if account.status == "active" {
                Color::Success
            } else {
                Color::Warning
            };
            let check_icon = if is_selected { "✓" } else { "○" };
            let check_color = if is_selected { Color::Accent } else { Color::Muted };

            let is_current = current_id.as_deref() == Some(account_id.as_str());

            let daily_pct = account.daily_percent.unwrap_or(100);
            let weekly_pct = account.weekly_percent.unwrap_or(100);
            let credit_color = if daily_pct < 20 {
                Color::Error
            } else if daily_pct < 50 {
                Color::Warning
            } else {
                Color::Success
            };
            let credit_label = SharedString::from(format!(
                "{}%↓ {}%/wk",
                daily_pct, weekly_pct
            ));
            let plan_label = account
                .plan_name
                .as_deref()
                .unwrap_or("")
                .to_string();
            let is_broken = account.ok_model_count == 0;
            let caps_label = if is_broken {
                SharedString::from("no models")
            } else {
                SharedString::from(format!("{} models", account.ok_model_count))
            };
            let caps_color = if is_broken { Color::Error } else { Color::Muted };

            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .py_1()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new(
                                SharedString::from(format!("chk-{}", account_id)),
                                check_icon,
                            )
                            .label_size(LabelSize::Small)
                            .color(check_color)
                            .on_click({
                                let id = account_id.clone();
                                cx.listener(move |this, _, _, cx| {
                                    this.state.update(cx, |state, cx| {
                                        state.toggle_selection(id.clone(), cx);
                                    });
                                })
                            }),
                        )
                        .when(is_current, |this| {
                            this.child(
                                Label::new("★")
                                    .size(LabelSize::Small)
                                    .color(Color::Accent),
                            )
                        })
                        .child(Label::new(account.email.clone()))
                        .child(
                            Label::new(account.tier.clone())
                                .size(LabelSize::Small)
                                .color(tier_color),
                        )
                        .child(
                            Label::new(account.status.clone())
                                .size(LabelSize::Small)
                                .color(status_color),
                        )
                        .child(
                            Label::new(credit_label)
                                .size(LabelSize::Small)
                                .color(credit_color),
                        )
                        .when(!plan_label.is_empty(), |this| {
                            this.child(
                                Label::new(plan_label)
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        })
                        .child(
                            Label::new(caps_label)
                                .size(LabelSize::Small)
                                .color(caps_color),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(
                                SharedString::from(format!("use-only-{}", account_id)),
                                if exclusive_id.as_deref() == Some(account_id.as_str()) {
                                    "◉ Sole"
                                } else {
                                    "Use Only"
                                },
                            )
                            .label_size(LabelSize::Small)
                            .color(if exclusive_id.as_deref() == Some(account_id.as_str()) {
                                Color::Accent
                            } else {
                                Color::Muted
                            })
                            .tooltip(ui::Tooltip::text(
                                "Remove all other accounts so only this one is used",
                            ))
                            .on_click({
                                let id = account_id.clone();
                                cx.listener(move |this, _, _, cx| {
                                    this.state
                                        .update(cx, |state, cx| {
                                            state.set_exclusive_account(id.clone(), cx)
                                        })
                                        .detach();
                                })
                            }),
                        )
                        .child(
                            Button::new(
                                SharedString::from(format!("del-{}", account_id)),
                                "Remove",
                            )
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_account(account_id.clone(), window, cx);
                            })),
                        ),
                )
        });

        let bulk_bar = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .when(exclusive_id.is_some(), |this| {
                let label = if restorable_count > 0 {
                    SharedString::from(format!("⟳ Auto ({} restorable)", restorable_count))
                } else {
                    SharedString::from("⟳ Auto (re-add manually)")
                };
                this.child(
                    Button::new("restore-all", label)
                        .label_size(LabelSize::Small)
                        .color(Color::Accent)
                        .tooltip(ui::Tooltip::text(
                            "Restore all paused accounts to rotation",
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state
                                .update(cx, |state, cx| state.restore_all_accounts(cx))
                                .detach();
                        }))
                )
            })
            .when(!exclusive_id.is_some(), |this| {
                this.child(
                    Button::new("sel-all", if all_selected { "Deselect All" } else { "Select All" })
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| {
                                if state.selected_account_ids.len() == state.proxy_accounts.len()
                                    && !state.proxy_accounts.is_empty()
                                {
                                    state.deselect_all_accounts(cx);
                                } else {
                                    state.select_all_accounts(cx);
                                }
                            });
                        }))
                )
            })
            .when(selected_count > 0 && exclusive_id.is_none(), |this| {
                this.child(
                    Button::new(
                        "del-selected",
                        SharedString::from(format!("Delete Selected ({})", selected_count)),
                    )
                    .label_size(LabelSize::Small)
                    .color(Color::Warning)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state
                            .update(cx, |state, cx| state.remove_selected_accounts(cx))
                            .detach();
                    })),
                )
            })
            .when(has_broken && exclusive_id.is_none(), |this| {
                this.child(
                    Button::new("rm-broken", "Remove Invalid")
                        .label_size(LabelSize::Small)
                        .color(Color::Error)
                        .tooltip(ui::Tooltip::text("Remove all accounts with no working models"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state
                                .update(cx, |state, cx| state.remove_broken_accounts(cx))
                                .detach();
                        }))
                )
            });

        let accounts_section = v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        Label::new(if accounts.is_empty() && paused_count == 0 {
                            "No accounts — add one below."
                        } else if exclusive_id.is_some() {
                            "Solo Mode — one active, others paused:"
                        } else {
                            "Accounts (round-robin load balanced):"
                        })
                        .size(LabelSize::Small)
                        .color(if exclusive_id.is_some() {
                            Color::Accent
                        } else {
                            Color::Muted
                        }),
                    )
                    .child(
                        Button::new(
                            "refresh-accounts",
                            if accounts_loading { "Refreshing…" } else { "Refresh" },
                        )
                        .label_size(LabelSize::Small)
                        .disabled(accounts_loading)
                        .on_click(cx.listener(Self::refresh_accounts)),
                    ),
            )
            .children(account_rows)
            .children(paused_rows)
            .when(!accounts.is_empty() || paused_count > 0, |this| this.child(bulk_bar));

        let add_form = v_flex()
            .w_full()
            .gap_2()
            .on_action(cx.listener(Self::add_account))
            .child(
                Label::new("Add Account (Email + Password):")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(self.email_editor.clone())
            .child(self.password_editor.clone())
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("add-account-btn", "Add Account")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let email =
                                    this.email_editor.read(cx).text(cx).trim().to_string();
                                let password =
                                    this.password_editor.read(cx).text(cx).trim().to_string();
                                if email.is_empty() || password.is_empty() {
                                    return;
                                }
                                this.email_editor
                                    .update(cx, |input, cx| input.set_text("", window, cx));
                                this.password_editor
                                    .update(cx, |input, cx| input.set_text("", window, cx));
                                this.state
                                    .update(cx, |state, cx| {
                                        state.add_email_account(email, password, cx)
                                    })
                                    .detach();
                            })),
                    )
                    .child(
                        Button::new("batch-paste-btn", "Paste & Add All")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let text = cx
                                    .read_from_clipboard()
                                    .and_then(|item| item.text().map(|t| t.to_string()))
                                    .unwrap_or_default();
                                let pairs = parse_batch_accounts(&text);
                                for (email, password) in pairs {
                                    this.state
                                        .update(cx, |state, cx| {
                                            state.add_email_account(email, password, cx)
                                        })
                                        .detach();
                                }
                            }))
                            .tooltip(ui::Tooltip::text(
                                "Copy lines like  email----password  then click here",
                            )),
                    ),
            )
            .when_some(add_error, |this, err| {
                this.child(
                    Label::new(err)
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
            });

        v_flex()
            .w_full()
            .gap_3()
            .child(accounts_section)
            .child(add_form)
            .into_any()
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

fn parse_batch_accounts(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            if let Some(pos) = line.find("----") {
                let email = line[..pos].trim().to_string();
                let password = line[pos + 4..].trim().to_string();
                if !email.is_empty() && !password.is_empty() {
                    return Some((email, password));
                }
            }
            if let Some(pos) = line.rfind(':') {
                let before = &line[..pos];
                if before.contains('@') {
                    let email = before.trim().to_string();
                    let password = line[pos + 1..].trim().to_string();
                    if !email.is_empty() && !password.is_empty() {
                        return Some((email, password));
                    }
                }
            }
            None
        })
        .collect()
}

async fn fetch_proxy_accounts(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
) -> Result<Vec<ProxyAccount>> {
    use futures::AsyncReadExt;
    let base_url = proxy_api_url
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let uri = format!("{base_url}/auth/accounts");
    let request = HttpRequest::builder()
        .method(Method::GET)
        .uri(uri)
        .body(AsyncBody::empty())?;
    let mut response = http_client.send(request).await?;
    if !response.status().is_success() {
        anyhow::bail!("accounts fetch failed: {}", response.status());
    }
    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let arr = json["accounts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("no accounts field"))?;
    let accounts = arr
        .iter()
        .filter_map(|a| {
            let credits = &a["credits"];
            let daily_percent = credits["dailyPercent"]
                .as_u64()
                .or_else(|| credits["percent"].as_u64())
                .map(|v| v as u32);
            let weekly_percent = credits["weeklyPercent"].as_u64().map(|v| v as u32);
            let plan_name = credits["planName"].as_str().map(|s| s.to_string());
            let last_used = a["lastUsed"].as_str().map(|s| {
                s.get(..19).unwrap_or(s).replace('T', " ")
            });
            let ok_model_count = a["capabilities"]
                .as_object()
                .map(|caps| caps.values().filter(|v| v["ok"].as_bool() == Some(true)).count() as u32)
                .unwrap_or(0);
            let error_count = a["errorCount"].as_u64().unwrap_or(0) as u32;
            Some(ProxyAccount {
                id: a["id"].as_str()?.to_string(),
                email: a["email"].as_str()?.to_string(),
                tier: a["tier"].as_str().unwrap_or("unknown").to_string(),
                status: a["status"].as_str().unwrap_or("unknown").to_string(),
                daily_percent,
                weekly_percent,
                plan_name,
                last_used,
                ok_model_count,
                error_count,
            })
        })
        .collect();
    Ok(accounts)
}

async fn add_email_account_to_proxy(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
    email: &str,
    password: &str,
) -> Result<()> {
    use futures::AsyncReadExt;
    let base_url = proxy_api_url
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let uri = format!("{base_url}/auth/login");
    let email_escaped = email.replace('"', "\\\"");
    let password_escaped = password.replace('"', "\\\"");
    let body = format!(
        "{{\"email\":\"{email_escaped}\",\"password\":\"{password_escaped}\"}}"
    );
    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(body))?;
    let mut response = http_client.send(request).await?;
    if !response.status().is_success() {
        let mut buf = String::new();
        response.body_mut().read_to_string(&mut buf).await.ok();
        let msg = serde_json::from_str::<serde_json::Value>(&buf)
            .ok()
            .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
            .unwrap_or(buf);
        anyhow::bail!("Login failed: {}", msg);
    }
    Ok(())
}

async fn remove_proxy_account(
    http_client: &dyn HttpClient,
    proxy_api_url: &str,
    account_id: &str,
) -> Result<()> {
    use futures::AsyncReadExt;
    let base_url = proxy_api_url
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let uri = format!("{base_url}/auth/accounts/{account_id}");
    let request = HttpRequest::builder()
        .method(Method::DELETE)
        .uri(uri)
        .body(AsyncBody::empty())?;
    let mut response = http_client.send(request).await?;
    if !response.status().is_success() {
        let mut buf = String::new();
        response.body_mut().read_to_string(&mut buf).await.ok();
        anyhow::bail!("Remove account failed: {}", buf);
    }
    Ok(())
}
