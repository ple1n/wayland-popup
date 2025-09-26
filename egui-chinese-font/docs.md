# egui-chinese-font Documentation

## API Reference

### Functions

#### `setup_chinese_fonts(ctx: &Context) -> Result<(), FontError>`

Automatically sets up Chinese fonts for an egui context by detecting and loading system fonts.

**Parameters:**
- `ctx`: The egui context to configure

**Returns:**
- `Ok(())` if fonts were successfully loaded
- `Err(FontError)` if font loading failed

**Example:**
```rust
use egui_chinese_font::setup_chinese_fonts;

let ctx = egui::Context::default();
setup_chinese_fonts(&ctx)?;
```

#### `setup_custom_chinese_font(ctx: &Context, font_data: Vec<u8>, font_name: Option<&str>)`

Sets up Chinese fonts using custom font data instead of system fonts.

**Parameters:**
- `ctx`: The egui context to configure
- `font_data`: The font data as bytes
- `font_name`: Optional name for the font (defaults to "chinese")

**Example:**
```rust
use egui_chinese_font::setup_custom_chinese_font;

let font_data = std::fs::read("path/to/font.ttf")?;
setup_custom_chinese_font(&ctx, font_data, Some("my-chinese-font"));
```

#### `get_chinese_font_paths() -> Vec<String>`

Returns a list of potential Chinese font paths on the current platform.

**Returns:**
- Vector of font paths as strings

**Example:**
```rust
use egui_chinese_font::get_chinese_font_paths;

let paths = get_chinese_font_paths();
for path in paths {
    println!("Font path: {}", path);
}
```

### Error Types

#### `FontError`

Enum representing different font loading errors.

**Variants:**
- `NotFound(String)`: Font file not found
- `ReadError(std::io::Error)`: Failed to read font file
- `UnsupportedPlatform`: Platform not supported

### Platform Support

#### Windows
Supports the following fonts:
- Microsoft YaHei (`msyh.ttc`, `msyhbd.ttc`)
- SimSun (`simsun.ttc`)
- SimHei (`simhei.ttf`)
- KaiTi (`simkai.ttf`)
- FangSong (`simfang.ttf`)
- Microsoft JhengHei (`msjh.ttc`, `msjhbd.ttc`)

#### macOS
Supports the following fonts:
- PingFang SC (`PingFang.ttc`)
- STHeiti (`STHeiti Light.ttc`, `STHeiti Medium.ttc`)
- Hiragino Sans GB (`Hiragino Sans GB.ttc`)
- Arial Unicode MS (`Arial Unicode.ttf`)

#### Linux
Supports the following fonts:
- Noto Sans CJK (`NotoSansCJK-Regular.ttc`)
- WQY MicroHei (`wqy-microhei.ttc`)
- WQY ZenHei (`wqy-zenhei.ttc`)
- AR PL UMing (`uming.ttc`, `ukai.ttc`)
- Droid Sans Fallback (`DroidSansFallbackFull.ttf`)

## Integration Guide

### Basic Setup

1. Add the dependency to your `Cargo.toml`:
```toml
[dependencies]
egui-chinese-font = "0.1"
egui = "0.27"
```

2. Call the setup function when creating your egui app:
```rust
use egui_chinese_font::setup_chinese_fonts;

eframe::run_native(
    "My App",
    options,
    Box::new(|cc| {
        setup_chinese_fonts(&cc.egui_ctx)?;
        Box::new(MyApp::default())
    }),
)
```

### Advanced Usage

For more control over font loading, you can use custom font data:

```rust
use egui_chinese_font::setup_custom_chinese_font;

// Load your own font file
let font_data = include_bytes!("../assets/my-chinese-font.ttf").to_vec();
setup_custom_chinese_font(&ctx, font_data, Some("custom-chinese"));
```

### Troubleshooting

If Chinese text is not displaying correctly:

1. Check if Chinese fonts are installed on your system
2. Use `get_chinese_font_paths()` to see available font paths
3. Try using `setup_custom_chinese_font()` with a known working font file
4. Check the console for error messages

### Performance Considerations

- Font loading happens once during application startup
- The library automatically selects the first available font from the list
- Font files are loaded into memory, so consider file sizes for embedded applications

## Examples

See the `examples/` directory for complete working examples:
- `basic.rs`: Basic usage with automatic font detection
