# this is not intended to be used.
mostly an experiment to learn Rust, how compositors work, and the low ;evel connection between the kernel and the screen. increasingly functional though :S

# what its for / the goal

tiling compositor for native wayland apps (master-stack layout). handles window layout, inputs, dropdowns/popups, workspaces (up to 999), drag & resize, layer shell (waybar works), screencopy (grim/slurp work), XWayland, animations, and a unix socket IPC at `/run/user/1000/vinland.sock`.

app borders might not be accurate. multi-monitor is not supported yet.

intended to be part of a small experimental DE

# howto

```
cargo build
cargo run
```

take note of your `WAYLAND_DISPLAY` — nested inside another compositor it'll be something like `wayland-1`.
to use tools like grim: `WAYLAND_DISPLAY=wayland-1 grim screenshot.png`

# pds

comes from the Vinland region in "Fear and Hunger"
:3c