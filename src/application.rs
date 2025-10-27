use std::{cell::RefCell, io::PipeReader, sync::Arc};

use crossbeam::queue::ArrayQueue;
use sctk::{
    reexports::calloop::{self, timer::Timer, EventLoop},
    shell::{
        wlr_layer::{Layer, SurfaceKind},
        WaylandSurface,
    },
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    layer_shell::{LayerShellOptions, WgpuLayerShellState},
    text_input::{ImeCapabilities, ImeEnableRequest},
    App, AppCreator, Result,
};

pub struct WgpuLayerShellApp {
    application: RefCell<Box<dyn App>>,
    pub event_loop: EventLoop<'static, WgpuLayerShellState>,
    pub layer_shell_state: WgpuLayerShellState,
}

#[derive(Debug)]
pub enum Msg {
    Toggle,
    Hide(bool),
    Passthrough(bool),
    Repaint,
    Exit,
}

#[derive(Debug)]
pub enum WPEvent {
    Fd(PipeReader),
}

pub type MsgQueue = calloop::channel::Sender<Msg>;
pub type EvRx = flume::Receiver<WPEvent>;

impl WgpuLayerShellApp {
    pub fn new(
        layer_shell_options: LayerShellOptions,
        app_creator: AppCreator,
    ) -> (MsgQueue, EvRx, Self) {
        let event_loop = EventLoop::try_new().expect("Could not create event loop.");
        let (sx, rx) = calloop::channel::channel::<Msg>();
        let (esx, erx) = flume::unbounded();
        let hd = event_loop.handle();
        let sx1 = sx.clone();

        event_loop
            .handle()
            .insert_source(rx, move |e, a, data: &mut WgpuLayerShellState| match e {
                calloop::channel::Event::Msg(m) => {
                    info!("{:?}", &m);
                    match m {
                        Msg::Toggle => {
                            if data.current_layer != Layer::Background {
                                data.current_layer = Layer::Background;
                                data.layer.set_layer(Layer::Background);
                                data.layer.commit();
                            } else {
                                data.current_layer = Layer::Overlay;
                                data.layer.set_layer(Layer::Overlay);
                                data.layer.commit();
                            }
                        }
                        Msg::Hide(b) => {
                            if b {
                                data.current_layer = Layer::Background;
                                data.layer.set_layer(Layer::Background);
                                data.layer.commit();
                            } else {
                                data.current_layer = Layer::Overlay;
                                data.layer.set_layer(Layer::Overlay);
                                data.layer.commit();
                            }
                        }
                        Msg::Passthrough(b) => {
                            data.set_passthrough(b);
                        }
                        Msg::Repaint => {
                            data.egui_state.context().request_repaint();
                        }
                        Msg::Exit => {
                            sx1.send(Msg::Hide(true)).unwrap();
                            hd.insert_source(Timer::immediate(), |e, m, d| {
                                std::process::exit(0);
                            })
                            .unwrap();
                        }
                    }
                }
                _ => (),
            })
            .unwrap();
        let layer_shell_state =
            WgpuLayerShellState::new(event_loop.handle(), layer_shell_options, esx);
        let app = RefCell::new(
            app_creator(&layer_shell_state.egui_state.context(), sx.clone(), erx.clone())
                .expect("could not create app"),
        );
        app.borrow().init(layer_shell_state.egui_state.context());
        (
            sx,
            erx,
            Self {
                // TODO: find better way to handle this potential error
                application: app,
                event_loop,
                layer_shell_state,
            },
        )
    }

    pub fn run_forever(mut self) -> Result {
        loop {
            self.event_loop
                .dispatch(
                    self.layer_shell_state.get_timeout(),
                    &mut self.layer_shell_state,
                )
                .unwrap();

            if self.layer_shell_state.should_draw() {
                let mut application = self.application.borrow_mut();
                self.layer_shell_state.draw(&mut **application);
            }

            // For some reason the layer get destroyed externally. Usually after resuming from computer suspension.
            if self.layer_shell_state.exit {
                warn!("layershell exited. restarting..");
                self.layer_shell_state = WgpuLayerShellState::new(
                    self.layer_shell_state.loop_handle,
                    self.layer_shell_state.layer_opts,
                    self.layer_shell_state.ev,
                );
                self.application
                    .borrow()
                    .init(self.layer_shell_state.egui_state.context());
            }
        }
        Ok(())
    }
}
