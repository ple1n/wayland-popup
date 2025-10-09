use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::io::pipe;
use std::os::fd::AsFd;
use std::os::fd::IntoRawFd;

use sctk::reexports::protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1, zwlr_data_control_manager_v1, zwlr_data_control_offer_v1,
    zwlr_data_control_source_v1,
};
use tracing::warn;
use wayland_client::{
    event_created_child,
    protocol::{wl_registry, wl_seat},
    Connection, Dispatch, Proxy,
};

use crate::application::WPEvent;
use crate::layer_shell::WgpuLayerShellState;

#[derive(Debug)]
pub enum WlListenType {
    ListenOnSelect,
    ListenOnCopy,
}

pub(crate) const TEXT: &str = "text/plain;charset=utf-8";
pub(crate) const IMAGE: &str = "image/png";

impl WgpuLayerShellState {
    fn is_text(&self) -> bool {
        !self.mime_types.is_empty()
            && self.mime_types.contains(&TEXT.to_string())
            && !self.mime_types.contains(&IMAGE.to_string())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        qh: &wayland_client::QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == wl_seat::WlSeat::interface().name {
                state.seat = Some(registry.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ()));
            } else if interface
                == zwlr_data_control_manager_v1::ZwlrDataControlManagerV1::interface().name
            {
                warn!("found ZwlrDataControlManagerV1");
                let mg = registry
                    .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(
                        name,
                        version,
                        qh,
                        (),
                    );

                state.data_device =
                    Some(mg.get_data_device(state.seat.as_ref().unwrap(), &state.queue_handle, ()));
                state.data_manager = Some(mg);
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        event: <wl_seat::WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Name { name } = event {
            state.seat_name = Some(name);
        }
    }
}

impl Dispatch<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, ()> for WgpuLayerShellState {
    fn event(
        _state: &mut Self,
        _proxy: &zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
        _event: <zwlr_data_control_manager_v1::ZwlrDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
        event: <zwlr_data_control_device_v1::ZwlrDataControlDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                if state.copy_data.is_some() {
                    return;
                }
                if let WlListenType::ListenOnSelect = state.listentype {
                    let (read, write) = pipe().unwrap();
                    state.current_type = Some(TEXT.to_string());
                    id.receive(TEXT.to_string(), write.as_fd());
                    drop(write);
                    state.pipereader = Some(read);
                }
            }
            zwlr_data_control_device_v1::Event::Finished => {
                let source = state
                    .data_manager
                    .as_ref()
                    .unwrap()
                    .create_data_source(qh, ());
                state
                    .data_device
                    .as_ref()
                    .unwrap()
                    .set_selection(Some(&source));
            }
            zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
                let Some(offer) = id else {
                    return;
                };
                let select_mimetype = |state: &WgpuLayerShellState| {
                    if state.is_text() || state.mime_types.is_empty() {
                        TEXT.to_string()
                    } else {
                        state.mime_types[0].clone()
                    }
                };
                let mimetype = if let Some(val) = &state.set_priority {
                    val.iter()
                        .find(|i| state.mime_types.contains(i))
                        .cloned()
                        .unwrap_or_else(|| select_mimetype(state))
                } else {
                    select_mimetype(state)
                };
                let (read, write) = std::io::pipe().unwrap();
                offer.receive(mimetype, write.as_fd());
                let _ = state.ev.send(WPEvent::Fd(read));
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                let Some(offer) = id else {
                    return;
                };
                if state.copy_data.is_some() {
                    return;
                }
                let select_mimetype = |state: &WgpuLayerShellState| {
                    if state.is_text() || state.mime_types.is_empty() {
                        TEXT.to_string()
                    } else {
                        state.mime_types[0].clone()
                    }
                };
                if let WlListenType::ListenOnCopy = state.listentype {
                    // if priority is set
                    let mimetype = if let Some(val) = &state.set_priority {
                        val.iter()
                            .find(|i| state.mime_types.contains(i))
                            .cloned()
                            .unwrap_or_else(|| select_mimetype(state))
                    } else {
                        select_mimetype(state)
                    };
                    state.current_type = Some(mimetype.clone());
                    let (read, write) = pipe().unwrap();
                    offer.receive(mimetype, write.as_fd());
                    drop(write);
                    state.pipereader = Some(read);
                }
            }
            _ => {
                log::info!("unhandled event: {event:?}");
            }
        }
    }
    event_created_child!(WgpuLayerShellState, zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ())
    ]);
}

impl Dispatch<zwlr_data_control_source_v1::ZwlrDataControlSourceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
        event: <zwlr_data_control_source_v1::ZwlrDataControlSourceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_source_v1::Event::Send { fd, mime_type } => {
                let Some(data) = state.copy_data.as_ref() else {
                    return;
                };
                // FIXME: how to handle the mime_type?
                if mime_type == TEXT || mime_type == IMAGE {
                    let mut f = File::from(fd);
                    f.write_all(&data.to_vec()).unwrap();
                }
            }
            zwlr_data_control_source_v1::Event::Cancelled => state.copy_cancelled = true,
            _ => {
                eprintln!("unhandled event: {event:?}");
            }
        }
    }
}

impl Dispatch<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
        event: <zwlr_data_control_offer_v1::ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.mime_types.push(mime_type);
        }
    }
}
