pub mod svg_compare;
pub mod wasm_backend;

pub fn init_test_backend() {
    if wasm_backend::wasm_backend_selected() {
        plantuml_little::layout::graphviz::set_custom_dot_renderer(|dot| {
            wasm_backend::render_dot_to_svg(dot)
        });
    }
}
