use std::{cell::RefCell, sync::Arc};

use crossbeam::queue::ArrayQueue;
use sctk::{
    reexports::calloop::{self, EventLoop},
    shell::{wlr_layer::Layer, WaylandSurface},
};
use tracing::info;

use crate::{
    layer_shell::{LayerShellOptions, WgpuLayerShellState},
    text_input::{ImeCapabilities, ImeEnableRequest},
    App, AppCreator, Result,
};

pub struct WgpuLayerShellApp {
    application: RefCell<Box<dyn App>>,
    pub event_loop: EventLoop<'static, WgpuLayerShellState>,
    layer_shell_state: WgpuLayerShellState,
}

#[derive(Debug)]
pub enum Msg {
    Hide(bool),
    Passthrough(bool),
    Repaint,
    Exit,
}

pub type MsgQueue = calloop::channel::Sender<Msg>;

impl WgpuLayerShellApp {
    pub fn new(
        layer_shell_options: LayerShellOptions,
        app_creator: AppCreator,
    ) -> (MsgQueue, Self) {
        let event_loop = EventLoop::try_new().expect("Could not create event loop.");
        let (sx, rx) = calloop::channel::channel::<Msg>();
        event_loop
            .handle()
            .insert_source(rx, |e, a, data: &mut WgpuLayerShellState| match e {
                calloop::channel::Event::Msg(m) => {
                    info!("{:?}", &m);
                    match m {
                        Msg::Hide(b) => {
                            if b {
                                data.layer.set_layer(Layer::Background);
                                data.layer.commit();
                            } else {
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
                            // Exiting here does not cause visual lag.
                            std::process::exit(0);
                        }
                    }
                }
                _ => (),
            })
            .unwrap();

        let layer_shell_state = WgpuLayerShellState::new(event_loop.handle(), layer_shell_options);
        (
            sx.clone(),
            Self {
                // TODO: find better way to handle this potential error
                application: RefCell::new(
                    app_creator(&layer_shell_state.egui_state.context(), sx)
                        .expect("could not create app"),
                ),
                event_loop,
                layer_shell_state,
            },
        )
    }

    pub fn run(&mut self) -> Result {
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

            if self.layer_shell_state.exit {
                println!("exiting example");
                break;
            }
        }
        Ok(())
    }
}
