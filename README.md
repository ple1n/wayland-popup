# template for making pop up like wayland apps

- [x] Acrylic semi-transparent windows that can instantly show or hide
- [ ] Root-based global input event listener
- [ ] Transparent overlay that tracks mouse? Doesnt seem to work

## dev

surface creation at 

    src/wgpu_state.rs
    src/layer_shell/mod.rs

wayland-clipboard-listener can be directly used, which gives a stream over, currrent text selection. 
It seems to require no extra privilege, not even extra supplemental gid. 