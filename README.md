# template for making pop up like wayland apps

- [x] Acrylic semi-transparent windows that can instantly show or hide
- [ ] Root-based global input event listener
- [ ] click passthrough
    - accidentally figured out how to do this. might even be a bug of the wayland server, because I am unsure if thats a part of wayland spec.
- [ ] IME

## dev

surface creation at 

    src/wgpu_state.rs
    src/layer_shell/mod.rs

wayland-clipboard-listener can be directly used, which gives a stream over, currrent text selection. 
It seems to require no extra privilege, not even extra supplemental gid. 

```
Caused by:
    Requested alpha mode PreMultiplied is not in the list of supported alpha modes: [Opaque]
```

I had no idea what part was responsible but I then installed vulkan-intel, and it got fixed. I was on Arch.

## bugs that ever happened

- currently, when there is vscode window behind the popup, the cursor position is offset by 4 pixels

## philosophy

- minimal dependency
- no config hierarchy, no such tree of config files with a priority list of include paths
- no dependency on systemd or anything
- no dBus

