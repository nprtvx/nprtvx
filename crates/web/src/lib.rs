//! Rust/WASM browser shell. Native clients can reuse `airms-core` and `airms-client`.

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let status = document.get_element_by_id("status").ok_or("missing status")?;
        status.set_text_content(Some("AirMS is ready — OpenMLS foundation loaded"));
        Ok(())
    }
}
