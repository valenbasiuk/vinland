//   - compositorhandler -> wl_surface commits
//   - shmhandler -> buffers de memoria compartida (wl_shm)
//   - bufferhandler -> buffer destruido por el cliente

use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::backend::renderer::utils::on_commit_buffer_handler;

use crate::state::{Vinland, ClientState};

// compositorhandler -> wl_surface commits
impl CompositorHandler for Vinland {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    // devuelve el estado del compositor para un cliente específico
    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        // registra el buffer del cliente en el estado interno de Smithay
        // sin esto, render_elements_from_surface_tree no puede importar el buffer
        on_commit_buffer_handler::<Self>(surface);

        // PopupManager.commit() mapea el popup (lo mueve de unmapped → mapped).
        // Después buscamos el popup para enviar el configure inicial si aún no fue enviado.
        // Este es el patrón correcto de Anvil: new_popup solo trackea, el commit configura.
        self.popups.commit(surface);
        if let Some(popup) = self.popups.find_popup(surface) {
            match popup {
                smithay::desktop::PopupKind::Xdg(ref xdg_popup) => {
                    if !xdg_popup.is_initial_configure_sent() {
                        // NOTE: El configure inicial siempre está permitido.
                        xdg_popup.send_configure().expect("popup initial configure failed");
                        tracing::info!("[compositor] popup configure inicial enviado para {:?}", surface.id());
                    }
                }
                _ => {}
            }
        }

        // Si la superficie pertenece a una de nuestras ventanas normales (sin padre)
        // y aún no ha sido configurada/tilada (su rect de tamaño es 0):
        let mut should_retile = false;
        for w in self.windows.iter_mut() {
            if w.surface.wl_surface() == surface
                && w.surface.parent().is_none()
                && w.rect.size.w == 0
                && w.rect.size.h == 0
            {
                // Establecemos un tamaño temporal de (1, 1) para marcarla como "lista para tilar"
                // y evitar que vuelva a dispararse en futuros commits
                w.rect.size = (1, 1).into();
                should_retile = true;
                break;
            }
        }

        if should_retile {
            self.retile();
        }

        // pedimos redibujo para que el próximo frame muestre el contenido del cliente
        self.backend.window().request_redraw();
    }
}

// shmhandler -> buffers de memoria compartida (wl_shm)
impl ShmHandler for Vinland {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm_state
    }
}

// bufferhandler -> buffer destruido por el cliente
impl BufferHandler for Vinland {
    fn buffer_destroyed(
        &mut self,
        _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
        // todo: liberar texturas importadas de este buffer.
    }
}
