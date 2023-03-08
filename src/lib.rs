use std::panic;

use wasm_bindgen::prelude::*;

mod hierarchy;
mod mesh;
mod orientation;
mod ply;
mod renderer;
mod stream;

/*
#[wasm_bindgen]
pub async fn run(root: web_sys::Element) {
    log::info!("Initializing flowguide...");

    // Get UI buttons
    let load_mesh: web_sys::HtmlButtonElement = root
        .children()
        .get_with_name("load_mesh")
        .and_then(|e| e.dyn_into().ok())
        .expect("Could not find mesh!");
    let preview = root
        .children()
        .get_with_name("preview")
        .and_then(|e| e.dyn_into().ok())
        .expect("Could not find preview!");

    // Initialize renderer
    let renderer = Renderer::new(preview)
        .await
        .expect("Could not create renderer");
    let renderer_proxy = renderer.proxy();

    // Attach UI events
    let load_mesh_handler = Closure::<dyn FnMut()>::new(move || {
        let mesh: web_sys::HtmlInputElement = root
            .children()
            .get_with_name("mesh")
            .and_then(|e| e.dyn_into().ok())
            .expect("Could not find mesh!");
        let renderer_proxy = renderer_proxy.clone();

        wasm_bindgen_futures::spawn_local(async move {
            if let Some(selected_file) = mesh.files().and_then(|list| list.item(0)) {
                log::info!("Loading {}...", selected_file.name());

                let js_reader = web_sys::ReadableStreamDefaultReader::new(&selected_file.stream())
                    .expect("Could not open file reader");
                let mut reader = AsyncStreamReader::new(move || {
                    let fut = js_reader.read();
                    async {
                        wasm_bindgen_futures::JsFuture::from(fut)
                            .await
                            .ok()
                            .and_then(|v| js_sys::Reflect::get(&v, &"value".into()).ok())
                            .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
                            .map(|a| a.to_vec())
                    }
                });

                let mut st = js_sys::Date::now();
                let model = load_ply(&mut reader).await.unwrap();
                log::info!(
                    "Read {} vertices, {} tris in {}ms",
                    model.vertices.len(),
                    model.tris.len(),
                    js_sys::Date::now() - st,
                );

                if let Err(_) = renderer_proxy.send_event(RendererEvent::UploadMesh(model.clone()))
                {
                    log::warn!("Failed to notify renderer of new mesh!");
                }

                st = js_sys::Date::now();
                let processed = ProcessMesh::from(model);
                log::info!("Processed mesh in {}ms", js_sys::Date::now() - st);

                st = js_sys::Date::now();
                let hierarchy = hierarchy::build(processed);
                log::info!("Built hierarchy in {}ms", js_sys::Date::now() - st);

                st = js_sys::Date::now();
                let o_field = orientation::hierarchical_smoothing(&hierarchy, 10);
                log::info!("Oriented mesh in {}ms", js_sys::Date::now() - st);

                if let Err(_) = renderer_proxy.send_event(RendererEvent::UploadOField(
                    hierarchy[hierarchy.len() - 1].mesh.vertices.clone(),
                    hierarchy[hierarchy.len() - 1].mesh.normals.clone(),
                    o_field,
                )) {
                    log::warn!("Failed to notify renderer of new mesh!");
                }
            }
        });
    });
    load_mesh.set_onclick(Some(load_mesh_handler.as_ref().unchecked_ref()));

    renderer.run();
}
*/

#[wasm_bindgen(start)]
pub fn main() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("Could not initialize logging");
}
