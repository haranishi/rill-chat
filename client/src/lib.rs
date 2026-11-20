mod api;
mod appearance;
mod demo;
mod dialogs;
mod dom;
mod model;
mod runtime;
mod view;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    appearance::before_paint();
    if dom::document().ready_state() == "loading" {
        dom::on(dom::document().as_ref(), "DOMContentLoaded", |_| mount());
    } else {
        mount();
    }
}

fn mount() {
    appearance::mount();
    dialogs::start();
    runtime::start();
}
