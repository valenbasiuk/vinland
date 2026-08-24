// state.rs
// struct central de vinland y constructor

use calloop::LoopSignal;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::backend::renderer::ImportMem;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::PopupManager;
use smithay::input::{keyboard::XkbConfig, pointer::CursorImageStatus, Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale as OutputScale, Subpixel};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::{Layer, LayerSurface, WlrLayerShellState};
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgShellState};
use smithay::wayland::shm::ShmState;
use smithay::wayland::xdg_foreign::XdgForeignState;
use smithay::wayland::xwayland_shell::XWaylandShellState;
use smithay::xwayland::{X11Wm, XWayland};

use crate::config::Config;

// window -> representa una ventana y su posición/tamaño en pantalla (tiling layout)
// tener surface y rect juntos = mejor cache locality que tenerlos en vecs separados
pub struct Window {
    pub surface: ToplevelSurface,
    pub rect: Rectangle<i32, Logical>,
    pub minimized: bool,
    pub floating: bool,
}

// workspace -> escritorio virtual, contiene sus propias ventanas
// el compositor mantiene un vec de workspaces y un índice activo.
// solo el workspace activo se renderiza y recibe input.
pub struct Workspace {
    pub windows: Vec<Window>,
}

pub struct LayerSurfaceItem {
    pub surface: LayerSurface,
    pub layer: Layer,
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
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub popups: PopupManager, // gestor de popups de Smithay (maneja jerarquía, posicionamiento y grabs)
    pub pointer_pos: Point<f64, Logical>,
    pub data_device_state: DataDeviceState,
    pub cursor_status: CursorImageStatus,
    pub xdg_foreign_state: XdgForeignState,
    pub layer_shell_state: WlrLayerShellState,
    // layer surfaces activas, ordenadas por capa (Background, Bottom, Top, Overlay)
    pub layer_surfaces: Vec<LayerSurfaceItem>,
    // textura GL del wallpaper (None si no hay wallpaper configurado)
    pub wallpaper_texture: Option<GlesTexture>,
    #[allow(dead_code)]
    pub xwayland: Option<XWayland>,
    pub xwm: Option<X11Wm>,
    pub xdisplay: Option<u32>,
    pub xwayland_shell_state: XWaylandShellState,
    pub config: Config,
}

/// Intenta cargar la imagen de fondo configurada y subirla como textura GL.
/// Retorna None si no hay wallpaper configurado o si falla la carga.
pub fn load_wallpaper(renderer: &mut GlesRenderer, config: &Config) -> Option<GlesTexture> {
    let path = config.background.wallpaper.as_ref()?;

    let reader = match image::ImageReader::open(path) {
        Ok(r) => match r.with_guessed_format() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("[wallpaper] error identificando formato de {:?}: {}", path, e);
                return None;
            }
        },
        Err(e) => {
            tracing::warn!("[wallpaper] no se pudo abrir {:?}: {}", path, e);
            return None;
        }
    };

    let img = match reader.decode() {
        Ok(i) => i.into_rgba8(),
        Err(e) => {
            tracing::warn!("[wallpaper] no se pudo decodificar {:?}: {}", path, e);
            return None;
        }
    };

    let (w, h) = img.dimensions();
    let data: Vec<u8> = img.into_raw();

    match renderer.import_memory(
        &data,
        smithay::backend::allocator::Fourcc::Abgr8888,
        smithay::utils::Size::from((w as i32, h as i32)),
        false, // no flipped
    ) {
        Ok(tex) => {
            tracing::info!("[wallpaper] textura cargada {}×{}", w, h);
            Some(tex)
        }
        Err(e) => {
            tracing::warn!("[wallpaper] error al subir textura: {:?}", e);
            None
        }
    }
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
                layout: &config.keyboard.layout,
                options: config.keyboard.options.clone(),
                ..XkbConfig::default()
            },
            config.keyboard.repeat_delay,
            config.keyboard.repeat_rate,
        )
        .unwrap();
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

        let xwayland_shell_state = XWaylandShellState::new::<Vinland>(&display_handle);
        let layer_shell_state = WlrLayerShellState::new::<Vinland>(&display_handle);

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
            layer_shell_state,
            layer_surfaces: Vec::new(),
            wallpaper_texture: None, // se carga en main.rs después de init
            // 9 workspaces vacíos, activo el 0 (índice base-0 = workspace 1 para el usuario)
            workspaces: (0..9).map(|_| Workspace { windows: Vec::new() }).collect(),
            active_workspace: 0,
            popups: PopupManager::default(),
            pointer_pos: (0.0, 0.0).into(),
            data_device_state,
            cursor_status: CursorImageStatus::default_named(),
            xwayland: None,
            xwm: None,
            xdisplay: None,
            xwayland_shell_state,
            config,
        };

        (state, winit_evt_loop)
    }

    // helpers para acceder a las ventanas del workspace activo
    pub fn windows(&self) -> &Vec<Window> {
        &self.workspaces[self.active_workspace].windows
    }

    pub fn windows_mut(&mut self) -> &mut Vec<Window> {
        &mut self.workspaces[self.active_workspace].windows
    }

    // retile -> calcula y envia la nueva disposicion tiling a todas las ventanas
    // ignora dialogos o ventanas que tienen padre (transient/floating)
    // tambien ignora ventanas normales temporales o auxiliares que no tienen buffer
    //
    // layout master-stack:
    //   gap | master | gap | stack_0 | gap
    //   gap |        | gap | stack_1 | gap
    //   ...                | stack_n | gap
    //
    // master_ratio controla que fraccion del ancho usable ocupa el master (0.0-1.0)
    pub fn retile(&mut self) {
        let gap = self.config.tiling.gap;
        let ratio = self.config.tiling.master_ratio;

        // contamos solo las ventanas sin padre y no flotantes que no esten minimizadas y que ya tengan buffer o ya fueron tiladas
        let tiled_count = self
            .windows()
            .iter()
            .filter(|w| {
                !w.minimized
                    && !w.floating
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

        // ancho usable = pantalla menos los bordes exteriores y el gap central
        // | gap | master | gap | stack | gap |
        // usable = w - 3*gap (si hay 2 columnas), o w - 2*gap (si hay 1 ventana)
        let mut tiled_idx = 0;
        let total_tiled = tiled_count;

        for win in self.windows_mut().iter_mut() {
            if win.minimized {
                continue;
            }
            if win.floating {
                continue;
            }
            if win.surface.parent().is_some() {
                continue;
            }
            let has_buffer =
                with_renderer_surface_state(win.surface.wl_surface(), |renderer_state| {
                    renderer_state.buffer().is_some()
                })
                .unwrap_or(false);
            if win.rect.size.w == 0 && !has_buffer {
                continue;
            }

            if total_tiled == 1 {
                // unica ventana: fullscreen con margen exterior en todos los bordes
                let win_w = w - gap * 2;
                let win_h = h - gap * 2;
                win.rect = Rectangle::new((gap, gap).into(), (win_w, win_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((win_w, win_h)));
                });
                win.surface.send_configure();
            } else if tiled_idx == 0 {
                // master: columna izquierda
                // x = gap, ancho = (w - 3*gap) * ratio
                let usable = w - gap * 3; // espacio entre bordes exteriores menos gap central
                let master_w = (usable as f32 * ratio) as i32;
                let win_h = h - gap * 2;
                win.rect = Rectangle::new((gap, gap).into(), (master_w, win_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((master_w, win_h)));
                });
                win.surface.send_configure();
                tiled_idx += 1;
            } else {
                // stack: columna derecha, dividida verticalmente
                let usable = w - gap * 3;
                let master_w = (usable as f32 * ratio) as i32;
                let stack_x = gap + master_w + gap; // borde izq + master + gap central
                let stack_w = w - stack_x - gap;    // hasta el borde derecho con gap
                let stack_count = total_tiled as i32 - 1;
                let stack_idx = tiled_idx as i32 - 1;

                // dividir la altura disponible en slots iguales con gap entre cada uno
                // altura usable = h - gap*(stack_count+1) (gap arriba, entre cada slot, abajo)
                let usable_h = h - gap * (stack_count + 1);
                let slot_h = usable_h / stack_count;
                let y = gap + stack_idx * (slot_h + gap);

                win.rect = Rectangle::new((stack_x, y).into(), (stack_w, slot_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((stack_w, slot_h)));
                });
                win.surface.send_configure();
                tiled_idx += 1;
            }
        }
    }
}
