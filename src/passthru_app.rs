use std::{cell::RefCell, sync::Arc};

use crossbeam::queue::ArrayQueue;
use smithay_client_toolkit::{reexports::calloop::{self, EventLoop}, shell::{WaylandSurface, wlr_layer::Layer}};

use crate::{
    App, AppCreator, Result, layer_shell::{LayerShellOptions, passthrough::PassthroughShell}
};

pub struct PassthruApp {
    application: RefCell<Box<dyn App>>,
    event_loop: EventLoop<'static, PassthroughShell>,
    layer_shell_state: PassthroughShell,
}

use crate::application::Msg;

pub type MsgQueue = calloop::channel::Sender<Msg>;

impl PassthruApp {
    pub fn new(
        layer_shell_options: LayerShellOptions,
        app_creator: AppCreator,
    ) -> (MsgQueue, Self) {
        let event_loop = EventLoop::try_new().expect("Could not create event loop.");
        let (sx, rx) = calloop::channel::channel::<Msg>();
        event_loop
            .handle()
            .insert_source(rx, |e, a, data: &mut PassthroughShell| match e {
                calloop::channel::Event::Msg(m) => match m {
                    Msg::Hide(b) => {
                        println!("hide {}", b);
                        if b {
                            data.layer.set_layer(Layer::Background);
                            data.layer.commit();
                        } else {
                            data.layer.set_layer(Layer::Overlay);
                            data.layer.commit();
                        }
                    }
                },
                _ => (),
            })
            .unwrap();
        let layer_shell_state = PassthroughShell::new(event_loop.handle(), layer_shell_options);

        (
            sx.clone(),
            Self {
                // TODO: find better way to handle this potential error
                application: RefCell::new(
                    app_creator(&layer_shell_state.egui_state.context())
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
