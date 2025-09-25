#![allow(unreachable_code)]

use application::WgpuLayerShellApp;
use layer_shell::LayerShellOptions;

use crate::{
    application::MsgQueue, layer_shell::passthrough::PassthroughShell, passthru_app::PassthruApp,
};

pub mod application;
pub(crate) mod egui_state;
pub mod layer_shell;
pub(crate) mod wgpu_state;

pub mod text_input;
pub mod errors;
pub mod passthru_app;
pub mod proto;

/// Short for `Result<T, eframe::Error>`.
pub type Result<T = (), E = anyhow::Error> = std::result::Result<T, E>;

pub type AppCreator = Box<dyn FnOnce(&egui::Context, MsgQueue) -> anyhow::Result<Box<dyn App>>>;

pub trait App {
    fn update(&mut self, ctx: &egui::Context);

    // fn save(&mut self, _storage: &mut dyn Storage) {}
    // fn on_exit(&mut self) {}
    // fn auto_save_interval(&self) -> std::time::Duration {
    //     std::time::Duration::from_secs(30)
    // }
    // fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
    //     egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
    // }
}

pub fn run_layer(
    options: LayerShellOptions,
    app_creator: AppCreator,
) -> (MsgQueue, WgpuLayerShellApp) {
    let (q, app) = WgpuLayerShellApp::new(options, app_creator);

    (q, app)
}

pub fn run_layer_pass(
    options: LayerShellOptions,
    app_creator: AppCreator,
) -> (MsgQueue, PassthruApp) {
    let (q, app) = PassthruApp::new(options, app_creator);

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
    }

    let (sx, e) = run_layer(
        options,
        Box::new(|a, b| Ok(Box::new(SimpleLayerWrapper { update_fun, msg: b }))),
    );

    (sx, e)
}

pub fn run_layer_simple_pass(
    options: LayerShellOptions,
    update_fun: impl FnMut(&egui::Context, &MsgQueue) + 'static,
) -> (MsgQueue, PassthruApp) {
    struct SimpleLayerWrapper<U> {
        update_fun: U,
        msg: MsgQueue,
    }

    impl<U: FnMut(&egui::Context, &MsgQueue) + 'static> App for SimpleLayerWrapper<U> {
        fn update(&mut self, ctx: &egui::Context) {
            (self.update_fun)(ctx, &self.msg);
        }
    }

    let (sx, e) = run_layer_pass(
        options,
        Box::new(|a, b| Ok(Box::new(SimpleLayerWrapper { update_fun, msg: b }))),
    );

    (sx, e)
}
