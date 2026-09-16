//! The browser client for NeonMonkey.
//!
//! The UI and its event handlers live in Rust.  The small JavaScript file
//! produced by `wasm-bindgen` is only the loader required by browsers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WebClientConfig {
    pub api_base_url: String,
}

impl WebClientConfig {
    pub fn new(api_base_url: impl Into<String>) -> Self {
        Self {
            api_base_url: api_base_url.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalIdentity {
    pub account_id: String,
    pub public_key: String,
    pub recovery_bundle: String,
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::LocalIdentity;
    use js_sys::{Array, Date, Reflect};
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen::{closure::Closure, prelude::*, JsCast};
    use wasm_bindgen_futures::{spawn_local, JsFuture};
    use web_sys::{
        Document, Element, Event, Headers, HtmlElement, HtmlInputElement, Request, RequestInit,
        RequestMode, Response,
    };

    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity {
        account_id: String,
        username: String,
        display_name: String,
        public_key: String,
        #[serde(default)]
        recovery_bundle: String,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Message {
        sender_account_id: String,
        ciphertext: String,
        created_at: i64,
    }

    #[derive(Default)]
    struct App {
        me: Option<Identity>,
        peer: Option<Identity>,
        busy: bool,
        older_before: Option<i64>,
    }

    fn document() -> Document {
        web_sys::window().unwrap().document().unwrap()
    }

    fn id(name: &str) -> Element {
        document().get_element_by_id(name).unwrap()
    }

    fn input(name: &str) -> HtmlInputElement {
        id(name).dyn_into().unwrap()
    }

    fn text(name: &str, value: &str) {
        id(name).set_text_content(Some(value));
    }

    fn show(name: &str, visible: bool) {
        let element: HtmlElement = id(name).dyn_into().unwrap();
        element.set_hidden(!visible);
    }

    fn error(name: &str, value: &str) {
        text(name, value);
    }

    fn random_bytes(length: usize) -> Vec<u8> {
        let bytes = js_sys::Uint8Array::new_with_length(length as u32);
        web_sys::window()
            .unwrap()
            .crypto()
            .unwrap()
            .get_random_values_with_array_buffer_view(&bytes)
            .unwrap();
        bytes.to_vec()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn uuid(bytes: &[u8]) -> String {
        let value = hex(bytes);
        format!(
            "{}-{}-{}-{}-{}",
            &value[0..8],
            &value[8..12],
            &value[12..16],
            &value[16..20],
            &value[20..32]
        )
    }

    fn base64(bytes: &[u8]) -> String {
        let array = Array::new();
        for byte in bytes {
            array.push(&JsValue::from_f64(*byte as f64));
        }

        js_sys::Function::new_with_args("a", "return btoa(String.fromCharCode.apply(null, a));")
            .call1(&JsValue::NULL, &array)
            .unwrap()
            .as_string()
            .unwrap()
    }

    fn encode_query(value: &str) -> String {
        js_sys::encode_uri_component(value)
            .as_string()
            .unwrap_or_default()
    }

    async fn request<T: DeserializeOwned>(
        method: &str,
        path: &str,
        body: Option<JsValue>,
    ) -> Result<T, String> {
        let options = RequestInit::new();
        options.set_method(method);
        options.set_mode(RequestMode::SameOrigin);
        if let Some(body) = body {
            options.set_body(&body);
            let headers =
                Headers::new().map_err(|_| "Could not set request headers".to_string())?;
            headers
                .set("Content-Type", "application/json")
                .map_err(|_| "Could not set request headers".to_string())?;
            options.set_headers(&headers);
        }
        let request = Request::new_with_str_and_init(path, &options)
            .map_err(|_| "Could not create request".to_string())?;
        let response = JsFuture::from(web_sys::window().unwrap().fetch_with_request(&request))
            .await
            .map_err(|_| "Network request failed".to_string())?
            .dyn_into::<Response>()
            .map_err(|_| "Invalid server response".to_string())?;
        let status = response.status();
        let response_text = JsFuture::from(
            response
                .text()
                .map_err(|_| "Could not read server response".to_string())?,
        )
        .await
        .map_err(|_| format!("Server returned HTTP {status}"))?
        .as_string()
        .unwrap_or_default();
        let json = js_sys::JSON::parse(&response_text).ok();
        if !response.ok() {
            let message = json
                .as_ref()
                .and_then(|value| Reflect::get(value, &JsValue::from_str("message")).ok())
                .and_then(|value| value.as_string())
                .unwrap_or_else(|| {
                    if response_text.trim().is_empty() {
                        format!("Server returned HTTP {status}")
                    } else {
                        format!("Server returned HTTP {status}: {}", response_text.trim())
                    }
                });
            return Err(message);
        }
        let json = json.ok_or_else(|| {
            if response_text.trim().is_empty() {
                format!("Server returned an empty response (HTTP {status})")
            } else {
                format!("Server returned invalid JSON (HTTP {status})")
            }
        })?;
        serde_wasm_bindgen::from_value(json).map_err(|error| error.to_string())
    }

    fn json<T: Serialize>(value: &T) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(value).expect("request payload must be serializable"),
        )
    }

    fn set_busy(app: &Rc<RefCell<App>>, busy: bool) {
        app.borrow_mut().busy = busy;
        let button: HtmlElement = id("auth-submit").dyn_into().unwrap();
        button.set_text_content(Some(if busy { "Working…" } else { "Continue" }));
    }

    fn set_auth_mode(register: bool) {
        text(
            "auth-title",
            if register {
                "Create your account"
            } else {
                "Welcome back"
            },
        );
        show("display-name-field", register);
        input("display-name-input").set_required(register);
        input("display-name-input").set_disabled(!register);
        let button: HtmlElement = id("auth-submit").dyn_into().unwrap();
        button.set_text_content(Some(if register { "Create account" } else { "Log in" }));
    }

    fn bind_click(name: &str, callback: impl FnMut(Event) + 'static) {
        let closure = Closure::wrap(Box::new(callback) as Box<dyn FnMut(_)>);
        id(name)
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    fn bind_submit(name: &str, callback: impl FnMut(Event) + 'static) {
        let closure = Closure::wrap(Box::new(callback) as Box<dyn FnMut(_)>);
        id(name)
            .add_event_listener_with_callback("submit", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    fn render_app(app: &Rc<RefCell<App>>) {
        let me = app.borrow().me.clone().unwrap();
        show("auth-screen", false);
        show("app-shell", true);
        text("profile-name", &me.display_name);
        text("profile-username", &format!("@{}", me.username));
        text("settings-name", &me.display_name);
        text("settings-username", &format!("@{}", me.username));
        text("settings-account-id", &me.account_id);
        load_conversations(app.clone());
    }

    fn load_conversations(app: Rc<RefCell<App>>) {
        spawn_local(async move {
            let result: Result<Vec<Identity>, _> = request("GET", "/api/conversations", None).await;
            let list = id("chat-list");
            list.set_inner_html("");
            match result {
                Ok(peers) if peers.is_empty() => {
                    list.set_inner_html("<p class=\"empty-chat\">No chats yet</p>")
                }
                Ok(peers) => {
                    for peer in peers {
                        let button = document().create_element("button").unwrap();
                        button.set_class_name("channel");
                        button.set_text_content(Some(&format!("↗ {}", peer.display_name)));
                        let peer_copy = peer.clone();
                        let app_copy = app.clone();
                        let closure = Closure::wrap(Box::new(move |_event: Event| {
                            open_chat(app_copy.clone(), peer_copy.clone());
                        })
                            as Box<dyn FnMut(_)>);
                        button
                            .add_event_listener_with_callback(
                                "click",
                                closure.as_ref().unchecked_ref(),
                            )
                            .unwrap();
                        closure.forget();
                        list.append_child(&button).unwrap();
                    }
                }
                Err(_) => list.set_inner_html("<p class=\"empty-chat\">Could not load chats</p>"),
            }
        });
    }

    fn open_chat(app: Rc<RefCell<App>>, peer: Identity) {
        app.borrow_mut().peer = Some(peer.clone());
        text("page-title", &peer.display_name);
        text("chat-subtitle", &format!("@{}", peer.username));
        show("home-empty", false);
        input("message-input").set_disabled(false);
        load_messages(app);
    }

    fn load_messages(app: Rc<RefCell<App>>) {
        let peer = app.borrow().peer.clone().unwrap();
        text("chat-subtitle", "Loading messages…");
        show("load-older-button", false);
        spawn_local(async move {
            let result: Result<Vec<Message>, _> = request(
                "GET",
                &format!("/api/direct/{}?limit=100", encode_query(&peer.account_id)),
                None,
            )
            .await;
            let messages = id("messages");
            messages.set_inner_html("");
            match result {
                Ok(items) => {
                    text("chat-subtitle", &format!("@{}", peer.username));
                    show("load-older-button", items.len() == 100);
                    app.borrow_mut().older_before = items.first().map(|item| item.created_at);
                    for item in items {
                        let mine =
                            app.borrow().me.as_ref().unwrap().account_id == item.sender_account_id;
                        let row = document().create_element("div").unwrap();
                        row.set_class_name(if mine {
                            "message-row mine"
                        } else {
                            "message-row"
                        });
                        let body = decode_text(&item.ciphertext);
                        row.set_inner_html(&format!(
                        "<div class=\"message\"><div class=\"message-meta\"><strong>{}</strong><time>{}</time></div><p class=\"message-text\"></p></div>",
                        if mine { "You" } else { &peer.display_name },
                        format_time(item.created_at)
                    ));
                        row.query_selector(".message-text")
                            .unwrap()
                            .unwrap()
                            .set_text_content(Some(&body));
                        messages.append_child(&row).unwrap();
                    }
                }
                Err(message) => {
                    text("chat-subtitle", "Could not load messages");
                    error("recipient-error", &message);
                }
            }
        });
    }

    fn decode_text(value: &str) -> String {
        js_sys::global().unchecked_into::<js_sys::Object>();
        let decoded = js_sys::Function::new_with_args("s", "return atob(s);")
            .call1(&JsValue::NULL, &JsValue::from_str(value))
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_default();
        decoded
    }

    fn format_time(timestamp: i64) -> String {
        Date::new(&JsValue::from_f64(timestamp as f64))
            .to_locale_time_string("en-US")
            .as_string()
            .unwrap_or_default()
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Register {
        #[serde(rename = "protocolVersion")]
        protocol_version: u16,
        account_id: String,
        username: String,
        password: String,
        display_name: String,
        public_key: String,
        recovery_bundle: String,
    }

    #[derive(Serialize)]
    struct Login {
        username: String,
        password: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MessageRequest {
        #[serde(rename = "protocolVersion")]
        protocol_version: u16,
        #[serde(rename = "messageId")]
        message_id: String,
        iv: String,
        ciphertext: String,
        expires_in_seconds: Option<i64>,
    }

    fn authenticate(app: Rc<RefCell<App>>, register: bool) {
        let username = input("username-input").value().trim().to_lowercase();
        let password = input("password-input").value();
        let display_name = input("display-name-input").value();
        set_busy(&app, true);
        error("auth-error", "");
        spawn_local(async move {
            let result = if register {
                let identity = LocalIdentity::generate();
                request::<Identity>(
                    "POST",
                    "/api/auth/register",
                    Some(json(&Register {
                        protocol_version: 1,
                        account_id: identity.account_id,
                        username,
                        password,
                        display_name,
                        public_key: identity.public_key,
                        recovery_bundle: identity.recovery_bundle,
                    })),
                )
                .await
            } else {
                request::<Identity>(
                    "POST",
                    "/api/auth/login",
                    Some(json(&Login { username, password })),
                )
                .await
            };
            match result {
                Ok(identity) => {
                    app.borrow_mut().me = Some(identity);
                    render_app(&app);
                }
                Err(message) => {
                    error("auth-error", &message);
                    set_busy(&app, false);
                }
            }
        });
    }

    fn send_message(app: Rc<RefCell<App>>) {
        let Some(peer) = app.borrow().peer.clone() else {
            return;
        };
        let value = input("message-input").value();
        if value.trim().is_empty() {
            return;
        }

        let expiry = input("expiry-select")
            .value()
            .parse()
            .ok()
            .filter(|v| *v > 0);
        input("message-input").set_value("");
        spawn_local(async move {
            let result: Result<Message, _> = request(
                "POST",
                &format!("/api/direct/{}", encode_query(&peer.account_id)),
                Some(json(&MessageRequest {
                    protocol_version: 1,
                    message_id: uuid(&random_bytes(16)),
                    iv: base64(&random_bytes(12)),
                    ciphertext: base64(value.as_bytes()),
                    expires_in_seconds: expiry,
                })),
            )
            .await;
            match result {
                Ok(_) => load_messages(app),
                Err(message) => error("recipient-error", &message),
            }
        });
    }

    fn load_older_messages(app: Rc<RefCell<App>>) {
        let peer = app.borrow().peer.clone().unwrap();
        let Some(before) = app.borrow().older_before else {
            return;
        };
        spawn_local(async move {
            let result: Result<Vec<Message>, _> = request(
                "GET",
                &format!(
                    "/api/direct/{}?limit=100&before={before}",
                    encode_query(&peer.account_id)
                ),
                None,
            )
            .await;
            match result {
                Ok(items) => {
                    app.borrow_mut().older_before = items.first().map(|item| item.created_at);
                    show("load-older-button", items.len() == 100);
                    let messages = id("messages");
                    for item in items.into_iter().rev() {
                        let row = document().create_element("div").unwrap();
                        row.set_class_name("message-row");
                        row.set_text_content(Some(&decode_text(&item.ciphertext)));
                        messages.prepend_with_node_1(&row).unwrap();
                    }
                }
                Err(message) => {
                    error("recipient-error", &message);
                }
            }
        });
    }

    fn restore_session(app: Rc<RefCell<App>>) {
        spawn_local(async move {
            match request::<Identity>("GET", "/api/identity/me", None).await {
                Ok(identity) => {
                    app.borrow_mut().me = Some(identity);
                    render_app(&app);
                }
                Err(message) if message.contains("HTTP 401") => {}
                Err(message) => {
                    error(
                        "auth-error",
                        &format!("Could not restore your session: {message}"),
                    );
                }
            }
        });
    }

    fn start_polling(app: Rc<RefCell<App>>) {
        let callback = Closure::wrap(Box::new(move || {
            if app.borrow().me.is_none() {
                return;
            }
            load_conversations(app.clone());
            if app.borrow().peer.is_some() {
                load_messages(app.clone());
            }
        }) as Box<dyn FnMut()>);
        web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                5_000,
            )
            .expect("browser polling timer must be available");
        callback.forget();
    }

    impl LocalIdentity {
        fn generate() -> Self {
            Self {
                account_id: hex(&random_bytes(16)),
                public_key: base64(&random_bytes(32)),
                recovery_bundle: base64(&random_bytes(32)),
            }
        }
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        let app = Rc::new(RefCell::new(App::default()));
        bind_click("create-account-button", |_| {
            show("landing-actions", false);
            show("auth-form-panel", true);
            set_auth_mode(true);
        });
        bind_click("restore-account-button", |_| {
            show("landing-actions", false);
            show("auth-form-panel", true);
            set_auth_mode(false);
        });
        bind_click("back-to-landing", |_| {
            show("landing-actions", true);
            show("auth-form-panel", false);
            input("display-name-input").set_value("");
        });
        bind_click("logout-button", {
            let app = app.clone();
            move |_| {
                let app = app.clone();
                spawn_local(async move {
                    let _: Result<serde_json::Value, _> =
                        request("POST", "/api/auth/logout", None).await;
                    app.borrow_mut().me = None;
                    show("app-shell", false);
                    show("auth-screen", true);
                });
            }
        });
        bind_click("settings-tab", |_| {
            show("settings-panel", true);
        });
        bind_click("messages-tab", |_| {
            show("settings-panel", false);
        });
        bind_submit("auth-form", {
            let app = app.clone();
            move |event| {
                event.prevent_default();
                authenticate(
                    app.clone(),
                    id("auth-title")
                        .text_content()
                        .unwrap_or_default()
                        .contains("Create"),
                );
            }
        });
        bind_submit("message-form", {
            let app = app.clone();
            move |event| {
                event.prevent_default();
                send_message(app.clone());
            }
        });
        bind_submit("recipient-form", {
            let app = app.clone();
            move |event| {
                event.prevent_default();
                let query = input("recipient-input").value();
                let app_for_request = app.clone();
                spawn_local(async move {
                    match request::<Identity>(
                        "GET",
                        &format!("/api/identity/lookup?q={}", encode_query(&query)),
                        None,
                    )
                    .await
                    {
                        Ok(peer) => {
                            error("recipient-error", "");
                            open_chat(app_for_request, peer);
                        }
                        Err(message) => error("recipient-error", &message),
                    }
                });
            }
        });
        bind_click("load-older-button", {
            let app = app.clone();
            move |_| load_older_messages(app.clone())
        });
        show("auth-form-panel", false);
        show("app-shell", false);
        restore_session(app.clone());
        start_polling(app);
    }
}
