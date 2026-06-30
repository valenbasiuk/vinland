use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;

pub struct Vinland {
    pub display_handle: smithay::reexports::wayland_server::DisplayHandle,
    pub compositor_state: CompositorState,
    pub shm_state: smithay::wayland::shm::ShmState,
    pub backend: WinitGraphicsBackend<GlesRenderer>,
}

impl ShmHandler for Vinland {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm_state
    }   // pixel buffer protocol 
}

impl BufferHandler for Vinland {
    fn buffer_destroyed(&mut self, buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer) {
        // All renderers can handle buffer destruction at this point.
        // Some parts of window management may also use this function.
    }
}

    // data de los clientes para el handler
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl smithay::reexports::wayland_server::backend::ClientData for ClientState {}

impl CompositorHandler for Vinland {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {}
}

//Delegados de las interfaces
smithay::delegate_dispatch2!(Vinland);
