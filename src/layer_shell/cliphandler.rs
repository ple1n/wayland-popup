use std::fs::File;
use std::io::pipe;
use std::io::Read;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::fd::IntoRawFd;

use sctk::reexports::protocols::ext::data_control::v1::client::ext_data_control_device_v1;
use sctk::reexports::protocols::ext::data_control::v1::client::ext_data_control_manager_v1;
use sctk::reexports::protocols::ext::data_control::v1::client::ext_data_control_offer_v1;
use sctk::reexports::protocols::ext::data_control::v1::client::ext_data_control_source_v1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_manager_v1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_offer_v1;
use sctk::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1;
use sctk::reexports::protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1, zwlr_data_control_manager_v1, zwlr_data_control_offer_v1,
    zwlr_data_control_source_v1,
};
use tracing::warn;
use wayland_client::protocol::wl_data_device_manager;
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

impl Dispatch<ext_data_control_manager_v1::ExtDataControlManagerV1, ()> for WgpuLayerShellState {
    fn event(
        _state: &mut Self,
        _proxy: &ext_data_control_manager_v1::ExtDataControlManagerV1,
        _event: <ext_data_control_manager_v1::ExtDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_data_control_device_v1::ExtDataControlDeviceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ext_data_control_device_v1::ExtDataControlDeviceV1,
        event: <ext_data_control_device_v1::ExtDataControlDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                if state.copy_data.is_some() {
                    return;
                }
                if let WlListenType::ListenOnSelect = state.listentype {
                    let (read, write) = pipe().unwrap();
                    state.current_type = Some(TEXT.to_string());
                    id.receive(TEXT.to_string(), write.as_fd());
                    drop(write);
                    let _ = state.ev.send(WPEvent::Fd(read));
                    warn!("ext_data_control_device_v1::Event::DataOffer");
                }
            }
            ext_data_control_device_v1::Event::Finished => {
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
            ext_data_control_device_v1::Event::PrimarySelection { id } => {
                if let Some(offer) = id {
                    let (read, write) = std::io::pipe().unwrap();
                    offer.receive(TEXT.to_string(), write.as_fd());
                    warn!("ext_data_control_device_v1 PrimarySelection");
                    let _ = state.ev.send(WPEvent::Fd(read));
                    offer.destroy();
                }
            }
            ext_data_control_device_v1::Event::Selection { id } => {
                let Some(offer) = id else {
                    return;
                };

                warn!("ext_data_control_device_v1::Event::Selection");
                // if is copying, not run this
                if state.copy_data.is_some() {
                    return;
                }
                // TODO: how can I handle the mimetype?
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
    event_created_child!(WgpuLayerShellState, ext_data_control_device_v1::ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ext_data_control_offer_v1::ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ext_data_control_source_v1::ExtDataControlSourceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ext_data_control_source_v1::ExtDataControlSourceV1,
        event: <ext_data_control_source_v1::ExtDataControlSourceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { fd, mime_type } => {
                let Some(data) = state.copy_data.as_ref() else {
                    return;
                };
                // FIXME: how to handle the mime_type?
                if mime_type == TEXT || mime_type == IMAGE {
                    let mut f = File::from(fd);
                    f.write_all(&data.to_vec()).unwrap();
                }
            }
            ext_data_control_source_v1::Event::Cancelled => state.copy_cancelled = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_data_control_offer_v1::ExtDataControlOfferV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ext_data_control_offer_v1::ExtDataControlOfferV1,
        event: <ext_data_control_offer_v1::ExtDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.mime_types.push(mime_type);
        }
    }
}

use zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1;

impl Dispatch<ZwpPrimarySelectionSourceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPrimarySelectionSourceV1,
        event: <ZwpPrimarySelectionSourceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
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
        use zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;

        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            warn!(name = name, interface = interface);
            if interface == ZwpPrimarySelectionDeviceManagerV1::interface().name {
                let mg = registry.bind::<ZwpPrimarySelectionDeviceManagerV1, _, _>(
                    name,
                    version,
                    qh,
                    (),
                );
                if state.zwp_data_dev.is_none() {
                    if let Some(seat) = &state.seat {
                        state.zwp_data_dev = Some(mg.get_device(&seat, &state.queue_handle, ()));
                    }
                }
            } else if interface == wl_data_device_manager::WlDataDeviceManager::interface().name {
                registry.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
                    name,
                    version,
                    qh,
                    (),
                );
            } else if interface == wl_seat::WlSeat::interface().name {
                warn!(interface, "found");
                if state.seat.is_none() {
                    state.seat =
                        Some(registry.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ()));
                    let seat = state.seat.as_ref().unwrap();
                    if let Some(mg) = &state.data_manager {
                        state.data_device = Some(mg.get_data_device(seat, &state.queue_handle, ()));
                    }
                }
            } else if interface
                == ext_data_control_manager_v1::ExtDataControlManagerV1::interface().name
            {
                warn!("bind ExtDataControlManagerV1");
                state.ext_data_manager = Some(
                    registry.bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(
                        name,
                        version,
                        qh,
                        (),
                    ),
                )
            } else if interface
                == zwlr_data_control_manager_v1::ZwlrDataControlManagerV1::interface().name
            {
                warn!(interface);
                if state.data_manager.is_none() {
                    let mg = registry
                        .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(
                            name,
                            version,
                            qh,
                            (),
                        );
                    state.data_manager = Some(mg);
                }
            } else {
                // tracing::warn!("registry ignored {} {}", interface, name)
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

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &wl_data_device_manager::WlDataDeviceManager,
        event: <wl_data_device_manager::WlDataDeviceManager as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceManagerV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPrimarySelectionDeviceManagerV1,
        event: <ZwpPrimarySelectionDeviceManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceV1, ()> for WgpuLayerShellState {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPrimarySelectionDeviceV1,
        event: <ZwpPrimarySelectionDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        use zwp_primary_selection_device_v1::Event;
        match event {
            Event::Selection { id } => {
                warn!("selection");
                // let Some(offer) = id else {
                //     return;
                // };
                // let (read, write) = std::io::pipe().unwrap();
                // offer.receive(TEXT.to_string(), write.as_fd());
                // warn!("ZwpPrimarySelectionDeviceV1 Selection");
                // let _ = state.ev.send(WPEvent::Fd(read));
                // offer.destroy();
            }
            Event::DataOffer { offer } => {
                let (read, write) = std::io::pipe().unwrap();
                offer.receive(TEXT.to_string(), write.as_fd());
                warn!("ZwpPrimarySelectionDeviceV1 DataOffer");
                let _ = state.ev.send(WPEvent::Fd(read));
                offer.destroy();
            }
            _ => {}
        }
    }
    event_created_child!(WgpuLayerShellState, ZwpPrimarySelectionDeviceV1, [
        zwp_primary_selection_device_v1::EVT_DATA_OFFER_OPCODE => (zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()),
        zwp_primary_selection_device_v1::EVT_SELECTION_OPCODE => (zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, ()),
    ]);
}

use crate::layer_shell::cliphandler::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1;

impl Dispatch<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()>
    for WgpuLayerShellState
{
    fn event(
        _state: &mut Self,
        _proxy: &ZwpPrimarySelectionOfferV1,
        _event: <ZwpPrimarySelectionOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
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
            zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
                let Some(offer) = id else {
                    return;
                };
                let (read, write) = std::io::pipe().unwrap();
                offer.receive(TEXT.to_string(), write.as_fd());
                warn!("ZwlrDataControlDeviceV1 PrimarySelection");
                let _ = state.ev.send(WPEvent::Fd(read));
                offer.destroy();
            }
            _ => {}
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
            _ => {}
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
