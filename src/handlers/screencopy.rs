// screencopy.rs
// implementacion del protocolo zwlr_screencopy_manager_v1
// permite a utilidades como grim, slurp y grabadores de pantalla capturar frames

use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
};
use smithay::reexports::wayland_server::{
    protocol::{wl_buffer::WlBuffer, wl_shm},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::utils::{Physical, Rectangle};

use crate::state::Vinland;

/// Datos asociados a cada frame creado por el cliente
pub struct FrameData {
    pub region: Rectangle<i32, Physical>,
    #[allow(dead_code)]
    pub overlay_cursor: bool,
}

/// Solicitud de screencopy pendiente de ser procesada en el render loop
pub struct PendingScreencopyFrame {
    pub frame: ZwlrScreencopyFrameV1,
    pub region: Rectangle<i32, Physical>,
    pub buffer: WlBuffer,
}

/// Estado global de screencopy en el compositor
#[derive(Default)]
pub struct ScreencopyState {
    pub pending_frames: Vec<PendingScreencopyFrame>,
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, (), Vinland> for Vinland {
    fn bind(
        _state: &mut Vinland,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Vinland>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, (), Vinland> for Vinland {
    fn request(
        state: &mut Vinland,
        _client: &Client,
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Vinland>,
    ) {
        match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output: _output,
            } => {
                let size = state.backend.window_size();
                let region = Rectangle::new((0, 0).into(), size);
                let frame_data = FrameData {
                    region,
                    overlay_cursor: overlay_cursor != 0,
                };
                let frame_res = data_init.init(frame, frame_data);

                // Notificar al cliente de los formatos shm soportados
                frame_res.buffer(
                    wl_shm::Format::Xrgb8888,
                    size.w as u32,
                    size.h as u32,
                    (size.w * 4) as u32,
                );
                frame_res.buffer(
                    wl_shm::Format::Argb8888,
                    size.w as u32,
                    size.h as u32,
                    (size.w * 4) as u32,
                );
                frame_res.buffer_done();
            }
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                output: _output,
                x,
                y,
                width,
                height,
            } => {
                let size = state.backend.window_size();
                let rx = x.clamp(0, size.w);
                let ry = y.clamp(0, size.h);
                let rw = width.clamp(1, (size.w - rx).max(1));
                let rh = height.clamp(1, (size.h - ry).max(1));

                let region = Rectangle::new((rx, ry).into(), (rw, rh).into());
                let frame_data = FrameData {
                    region,
                    overlay_cursor: overlay_cursor != 0,
                };
                let frame_res = data_init.init(frame, frame_data);

                frame_res.buffer(
                    wl_shm::Format::Xrgb8888,
                    rw as u32,
                    rh as u32,
                    (rw * 4) as u32,
                );
                frame_res.buffer(
                    wl_shm::Format::Argb8888,
                    rw as u32,
                    rh as u32,
                    (rw * 4) as u32,
                );
                frame_res.buffer_done();
            }
            zwlr_screencopy_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameData, Vinland> for Vinland {
    fn request(
        state: &mut Vinland,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &FrameData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Vinland>,
    ) {
        match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer }
            | zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => {
                tracing::info!("[screencopy] cliente solicitó copia de región {:?}", data.region);
                state
                    .screencopy_state
                    .pending_frames
                    .push(PendingScreencopyFrame {
                        frame: frame.clone(),
                        region: data.region,
                        buffer,
                    });
                state.backend.window().request_redraw();
            }
            zwlr_screencopy_frame_v1::Request::Destroy => {}
            _ => {}
        }
    }
}
