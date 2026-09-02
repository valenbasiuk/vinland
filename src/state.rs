// state.rs
// struct central de vinland y constructor

use calloop::LoopSignal;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::ImportMem;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::PopupManager;
use smithay::input::{keyboard::XkbConfig, pointer::CursorImageStatus, Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale as OutputScale, Subpixel};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point, Rectangle, Serial, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::{Layer, LayerSurface, WlrLayerShellState};
use smithay::wayland::shell::xdg::{
    decoration::XdgDecorationState, ToplevelSurface, XdgShellState,
};
use smithay::wayland::shm::ShmState;
use smithay::wayland::xdg_foreign::XdgForeignState;
use smithay::wayland::xwayland_shell::XWaylandShellState;
use smithay::xwayland::{X11Wm, XWayland};

use crate::config::Config;

// objetivo de una captura de pantalla nativa
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenshotTarget {
    FullScreen,
    Window(Rectangle<i32, Logical>),
}

// estado de arrastre interactivo (tiling swap o movimiento de ventana flotante)
#[derive(Debug, Clone, PartialEq)]
pub enum DragState {
    None,
    TileSwap {
        source_surface: WlSurface,
        start_pos: Point<f64, Logical>,
    },
    FloatMove {
        source_surface: WlSurface,
        grab_offset: Point<f64, Logical>,
    },
}

// window -> representa una ventana y su posición/tamaño en pantalla (tiling layout)
// tener surface y rect juntos = mejor cache locality que tenerlos en vecs separados
pub struct Window {
    pub surface: ToplevelSurface,
    pub rect: Rectangle<i32, Logical>,
    pub minimized: bool,
    pub floating: bool,
    pub tile_order: usize,
    pub rules_evaluated: bool,
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
    #[allow(dead_code)]
    pub xdg_decoration_state: XdgDecorationState,
    pub super_pressed: bool,
    pub last_pointer_serial: Option<Serial>,
    pub drag_state: DragState,
    pub pending_screenshot: Option<ScreenshotTarget>,
    pub screenshot_flash_frames: u8,
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
    let raw = img.into_raw();

    match renderer.import_memory(
        &raw,
        smithay::backend::allocator::Fourcc::Abgr8888,
        (w as i32, h as i32).into(),
        false,
    ) {
        Ok(tex) => {
            tracing::info!("[wallpaper] textura cargada {}×{}", w, h);
            Some(tex)
        }
        Err(e) => {
            tracing::warn!("[wallpaper] error importando textura GL: {}", e);
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
        let xdg_decoration_state = XdgDecorationState::new::<Vinland>(&display_handle);

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
            xdg_decoration_state,
            super_pressed: false,
            last_pointer_serial: None,
            drag_state: DragState::None,
            pending_screenshot: None,
            screenshot_flash_frames: 0,
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

    // reload_config -> aplica una nueva configuracion en caliente
    // actualiza el config, recarga wallpaper si cambio, retila ventanas y redibuja
    pub fn reload_config(&mut self, new_config: crate::config::Config) {
        let wallpaper_changed = self.config.background.wallpaper != new_config.background.wallpaper
            || self.config.background.wallpaper_mode != new_config.background.wallpaper_mode;

        self.config = new_config;

        // recargar wallpaper si la ruta o modo cambiaron
        if wallpaper_changed {
            let (renderer, _fb) = self.backend.bind().expect("bind para reload wallpaper");
            self.wallpaper_texture = load_wallpaper(renderer, &self.config);
        }

        // retile con los nuevos gaps y master_ratio
        self.retile();

        // forzar redibujo inmediato para reflejar colores de bordes, wallpaper, etc.
        self.backend.window().request_redraw();
        tracing::info!("[hot-reload] configuracion recargada exitosamente");
    }

    // retile -> calcula y envia la nueva disposicion tiling a todas las ventanas
    // ignora dialogos o ventanas que tienen padre (transient/floating)
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

        let scale_factor = self.backend.scale_factor();
        let out_size = self
            .backend
            .window_size()
            .to_f64()
            .to_logical(scale_factor)
            .to_i32_round();
        let w: i32 = out_size.w;
        let h: i32 = out_size.h;

        // recopilar los indices de ventanas tilables ordenadas por su tile_order estable
        let mut tiled_indices: Vec<usize> = self
            .windows()
            .iter()
            .enumerate()
            .filter(|(_, win)| !win.minimized && !win.floating && win.surface.parent().is_none())
            .map(|(idx, _)| idx)
            .collect();

        if tiled_indices.is_empty() {
            return;
        }

        tiled_indices.sort_by_key(|&idx| self.windows()[idx].tile_order);
        let total_tiled = tiled_indices.len();

        for (tiled_idx, win_idx) in tiled_indices.into_iter().enumerate() {
            let win = &mut self.windows_mut()[win_idx];
            if total_tiled == 1 {
                // unica ventana: fullscreen con margen exterior en todos los bordes
                let win_w = w - gap * 2;
                let win_h = h - gap * 2;
                win.rect = Rectangle::new((gap, gap).into(), (win_w, win_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((win_w, win_h)));
                    s.states.set(xdg_toplevel::State::TiledTop);
                    s.states.set(xdg_toplevel::State::TiledBottom);
                    s.states.set(xdg_toplevel::State::TiledLeft);
                    s.states.set(xdg_toplevel::State::TiledRight);
                });
                win.surface.send_configure();
            } else if tiled_idx == 0 {
                // master: columna izquierda
                let usable = w - gap * 3;
                let master_w = (usable as f32 * ratio) as i32;
                let win_h = h - gap * 2;
                win.rect = Rectangle::new((gap, gap).into(), (master_w, win_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((master_w, win_h)));
                    s.states.set(xdg_toplevel::State::TiledTop);
                    s.states.set(xdg_toplevel::State::TiledBottom);
                    s.states.set(xdg_toplevel::State::TiledLeft);
                    s.states.set(xdg_toplevel::State::TiledRight);
                });
                win.surface.send_configure();
            } else {
                // stack: columna derecha, dividida verticalmente
                let usable = w - gap * 3;
                let master_w = (usable as f32 * ratio) as i32;
                let stack_x = gap + master_w + gap;
                let stack_w = w - stack_x - gap;
                let stack_count = total_tiled as i32 - 1;
                let stack_idx = tiled_idx as i32 - 1;

                let usable_h = h - gap * (stack_count + 1);
                let slot_h = usable_h / stack_count;
                let y = gap + stack_idx * (slot_h + gap);

                win.rect = Rectangle::new((stack_x, y).into(), (stack_w, slot_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((stack_w, slot_h)));
                    s.states.set(xdg_toplevel::State::TiledTop);
                    s.states.set(xdg_toplevel::State::TiledBottom);
                    s.states.set(xdg_toplevel::State::TiledLeft);
                    s.states.set(xdg_toplevel::State::TiledRight);
                });
                win.surface.send_configure();
            }
        }
    }
}
