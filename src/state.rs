// state.rs
// struct central de vinland y constructor

use calloop::LoopSignal;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::PopupManager;
use smithay::input::{keyboard::XkbConfig, pointer::CursorImageStatus, Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale as OutputScale, Subpixel};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgShellState};
use smithay::wayland::shm::ShmState;
use smithay::wayland::xdg_foreign::XdgForeignState; // parentezco apps

use crate::config::Config;

// window -> representa una ventana y su posición/tamaño en pantalla (tiling layout)
// tener surface y rect juntos = mejor cache locality que tenerlos en vecs separados
pub struct Window {
    pub surface: ToplevelSurface,
    pub rect: Rectangle<i32, Logical>,
    pub minimized: bool,
}
// vinland -> estado global del compositor
// todos los handlers de protocolos reciben &mut self de este struct
pub struct Vinland {
    pub display_handle: DisplayHandle,
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub xdg_shell_state: XdgShellState,
    pub seat_state: SeatState<Vinland>,
    pub seat: Seat<Vinland>,
    pub output: Output,
    pub backend: WinitGraphicsBackend<GlesRenderer>,
    pub loop_signal: LoopSignal,
    pub windows: Vec<Window>,
    pub popups: PopupManager,        // gestor de popups de Smithay (maneja jerarquía, posicionamiento y grabs)
    pub pointer_pos: Point<f64, Logical>,
    pub data_device_state: DataDeviceState,
    pub cursor_status: CursorImageStatus,
    pub xdg_foreign_state: XdgForeignState,
    pub config: Config,
}

// clientstate -> datos por cliente conectado
// smithay los almacena internamente y los provee en client_compositor_state()
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl smithay::reexports::wayland_server::backend::ClientData for ClientState {}

// delegate_dispatch2! -> genera el boilerplate de despacho de mensajes wayland
// conecta cada tipo de mensaje wayland al handler correcto en vinland
smithay::delegate_dispatch2!(Vinland);

impl Vinland {
    // new() -> crea e inicializa el compositor completo
    // toma &display para que main.rs pueda moverlo al generic source de calloop
    pub fn new(
        display: &Display<Vinland>,
        loop_signal: LoopSignal,
        config: Config,
    ) -> (
        Self,
        impl calloop::EventSource<Event = smithay::backend::winit::WinitEvent, Metadata = (), Ret = ()>,
    ) {
        let display_handle = display.handle();

        // protocolos wayland
        let compositor_state = CompositorState::new::<Vinland>(&display_handle);
        let shm_state = ShmState::new::<Vinland>(&display_handle, vec![]);
        let xdg_shell_state = XdgShellState::new::<Vinland>(&display_handle);
        let data_device_state = DataDeviceState::new::<Vinland>(&display_handle);
        let xdg_foreign_state = XdgForeignState::new::<Vinland>(&display_handle);

        // seat (inputs generales)
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&display_handle, "vinland-seat");
        seat.add_keyboard(
            XkbConfig {
                layout:  &config.keyboard.layout,
                options: config.keyboard.options.clone(),
                ..XkbConfig::default()
            },
            config.keyboard.repeat_delay,
            config.keyboard.repeat_rate,
        ).unwrap();
        seat.add_pointer();

        // backend de winit -> renderiza dentro de una ventana del compositor host
        // se inicializa ANTES del output para poder leer el tamaño real de la ventana
        let (backend, winit_evt_loop) = smithay::backend::winit::init::<GlesRenderer>()
            .expect("fallo al inicializar el backend de winit");

        // output: usamos el tamaño real de la ventana Winit para que los clientes
        // no vean un output distinto al espacio que tienen disponible
        let actual_size = backend.window_size();
        let mode = Mode {
            size: actual_size,
            refresh: 144000,
        };
        let output = Output::new(
            "vinland-output".into(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Vinland".into(),
                model: "Virtual".into(),
                serial_number: "".into(),
            },
        );
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            Some(OutputScale::Fractional(backend.scale_factor())),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.create_global::<Vinland>(&display_handle);

        let state = Vinland {
            display_handle,
            compositor_state,
            shm_state,
            xdg_shell_state,
            seat_state,
            seat,
            output,
            backend,
            loop_signal,
            xdg_foreign_state,
            windows: Vec::new(),
            popups: PopupManager::default(),
            pointer_pos: (0.0, 0.0).into(),
            data_device_state,
            cursor_status: CursorImageStatus::default_named(),
            config,
        };

        (state, winit_evt_loop)
    }

    // retile -> calcula y envía la nueva disposición tiling a todas las ventanas
    // ignora diálogos o ventanas que tienen padre (transient/floating)
    // también ignora ventanas normales temporales o auxiliares que no tienen buffer
    pub fn retile(&mut self) {
        let gap = self.config.tiling.gap;

        // contamos solo las ventanas sin padre que no estén minimizadas y que ya tengan un buffer o ya fueron tiladas (rect w > 0)
        let tiled_count = self
            .windows
            .iter()
            .filter(|w| {
                !w.minimized
                    && w.surface.parent().is_none()
                    && (w.rect.size.w > 0
                        || with_renderer_surface_state(w.surface.wl_surface(), |renderer_state| {
                            renderer_state.buffer().is_some()
                        })
                        .unwrap_or(false))
            })
            .count();

        if tiled_count == 0 {
            return;
        }

        let scale_factor = self.backend.scale_factor();
        let out_size = self
            .backend
            .window_size()
            .to_f64()
            .to_logical(scale_factor)
            .to_i32_round();
        let w: i32 = out_size.w;
        let h: i32 = out_size.h;

        let mut tiled_idx = 0;
        let total_tiled = tiled_count;

        for win in self.windows.iter_mut() {
            // si está minimizada, no la tilamos
            if win.minimized {
                continue;
            }

            // si tiene padre, dejamos que flote en su rect actual
            if win.surface.parent().is_some() {
                continue;
            }

            // si no tiene rect aún y tampoco tiene buffer, no lo tilamos todavía
            let has_buffer =
                with_renderer_surface_state(win.surface.wl_surface(), |renderer_state| {
                    renderer_state.buffer().is_some()
                })
                .unwrap_or(false);

            if win.rect.size.w == 0 && !has_buffer {
                continue;
            }

            if total_tiled == 1 {
                // única ventana: fullscreen con margen exterior
                win.rect = Rectangle::new(
                    (gap, gap).into(),
                    (w - gap * 2, h - gap * 2).into(),
                );
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((w - gap * 2, h - gap * 2)));
                });
                win.surface.send_configure();
            } else if tiled_idx == 0 {
                // master: mitad izquierda con gaps
                let master_w = w / 2 - gap - gap / 2;
                win.rect = Rectangle::new(
                    (gap, gap).into(),
                    (master_w, h - gap * 2).into(),
                );
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((master_w, h - gap * 2)));
                });
                win.surface.send_configure();
                tiled_idx += 1;
            } else {
                // stack: mitad derecha dividida con gaps
                let master_w = w / 2;
                let stack_x = master_w + gap;
                let stack_w = w - stack_x - gap;
                let slot_h = h / (total_tiled as i32 - 1);
                let stack_idx = tiled_idx - 1;
                let y = slot_h * stack_idx + gap;
                let slot_h_gapped = slot_h - gap * 2;

                win.rect = Rectangle::new(
                    (stack_x, y).into(),
                    (stack_w, slot_h_gapped).into(),
                );
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((stack_w, slot_h_gapped)));
                });
                win.surface.send_configure();
                tiled_idx += 1;
            }
        }
    }
}
