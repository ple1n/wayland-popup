#![allow(unreachable_code)]

use application::WgpuLayerShellApp;
use egui::{Color32, Visuals};
use layer_shell::LayerShellOptions;

use crate::{
    application::{EvRx, MsgQueue},
    layer_shell::WgpuLayerShellState,
};

pub mod application;
pub(crate) mod egui_state;
pub mod layer_shell;
pub(crate) mod wgpu_state;
pub use egui_chinese_font;
pub mod errors;
pub mod proto;
pub mod text_input;
pub use async_bincode;
pub use egui;
pub use exponential_backoff;
pub use flume;
pub use eframe;

/// Short for `Result<T, eframe::Error>`.
pub type Result<T = (), E = anyhow::Error> = std::result::Result<T, E>;

pub type AppCreator =
    Box<dyn FnOnce(&egui::Context, MsgQueue, EvRx) -> anyhow::Result<Box<dyn App>>>;

pub trait App {
    /// Handle UI state change
    fn sync(&mut self, layer: &WgpuLayerShellState);
    fn update(&mut self, ctx: &egui::Context);

    // fn save(&mut self, _storage: &mut dyn Storage) {}
    // fn on_exit(&mut self) {}
    // fn auto_save_interval(&self) -> std::time::Duration {
    //     std::time::Duration::from_secs(30)
    // }
    // fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
    //     egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
    // }
    fn init(&self, ctx: &egui::Context, layer: &WgpuLayerShellState) {}
}

pub fn run_layer(
    options: LayerShellOptions,
    app_creator: AppCreator,
) -> (MsgQueue, WgpuLayerShellApp) {
    let (q, r, app) = WgpuLayerShellApp::new(options, app_creator);

    (q, app)
}

pub fn run_layer_simple(
    options: LayerShellOptions,
    update_fun: impl FnMut(&egui::Context, &MsgQueue) + 'static,
) -> (MsgQueue, WgpuLayerShellApp) {
    struct SimpleLayerWrapper<U> {
        update_fun: U,
        msg: MsgQueue,
    }

    impl<U: FnMut(&egui::Context, &MsgQueue) + 'static> App for SimpleLayerWrapper<U> {
        fn update(&mut self, ctx: &egui::Context) {
            (self.update_fun)(ctx, &self.msg);
        }
        fn sync(&mut self, layer: &WgpuLayerShellState) {}
    }

    let (sx, e) = run_layer(
        options,
        Box::new(|a, b, c| Ok(Box::new(SimpleLayerWrapper { update_fun, msg: b }))),
    );

    (sx, e)
}

pub fn run_layer_cjk(
    options: LayerShellOptions,
    update_fun: impl FnMut(&egui::Context, &MsgQueue) + 'static,
) -> (MsgQueue, WgpuLayerShellApp) {
    struct SimpleLayerWrapper<U> {
        update_fun: U,
        msg: MsgQueue,
    }

    let mut li = Visuals::dark();
    li.override_text_color = Some(Color32::WHITE.gamma_multiply(0.7));

    impl<U: FnMut(&egui::Context, &MsgQueue) + 'static> App for SimpleLayerWrapper<U> {
        fn update(&mut self, ctx: &egui::Context) {
            (self.update_fun)(ctx, &self.msg);
        }
        fn sync(&mut self, layer: &WgpuLayerShellState) {}
    }

    let (sx, e) = run_layer(
        options,
        Box::new(|a, b, c| Ok(Box::new(SimpleLayerWrapper { update_fun, msg: b }))),
    );

    (sx, e)
}
