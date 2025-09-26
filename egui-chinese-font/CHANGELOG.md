# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-06-25

### Added
- Initial release of egui-chinese-font
- Cross-platform Chinese font loading for Windows, macOS, and Linux
- Automatic system font detection and loading
- Support for custom font data loading
- Comprehensive error handling with `FontError` enum
- API functions: `setup_chinese_fonts`, `setup_custom_chinese_font`, `get_chinese_font_paths`
- Support for multiple Chinese font formats (Simplified and Traditional)
- Platform-specific font prioritization
- Documentation and examples

### Features
- Windows: Support for Microsoft YaHei, SimSun, SimHei, KaiTi, FangSong, Microsoft JhengHei
- macOS: Support for PingFang SC, STHeiti, Hiragino Sans GB, Arial Unicode MS
- Linux: Support for Noto Sans CJK, WQY MicroHei, Droid Sans Fallback, AR PL UMing

[Unreleased]: https://github.com/username/egui-chinese-font/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/username/egui-chinese-font/releases/tag/v0.1.0
