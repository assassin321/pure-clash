//! Linux 字体回退与彩色 Emoji 旗帜渲染修补。
//!
//! GPUI 的 Linux 文本系统（`gpui_wgpu::CosmicTextSystem` + `cosmic-text 0.19`）存在三处会导致
//! 代理节点名称中的 Unicode 旗帜（如 `🇭🇰`）退化为双字母（`HK`）的限制：
//! 1. `.SystemUIFont` 在 Linux 上硬编码映射为 `"IBM Plex Sans"`；系统未安装该字体时，
//!    `TextSystem::resolve_font` 会丢弃字重（`font_weight`）和用户回退链（`font_fallbacks`），
//!    直接退回内置静态列表（甚至误中 WPS 安装的斜体 `Segoe UI`）。
//! 2. `cosmic-text` 的 Unix 默认回退链将 `"FreeSans"` 排在 `"Noto Color Emoji"` 之前，而
//!    Ubuntu/Debian 的 `FreeSans.ttf` 包含 `U+1F1E6..U+1F1FF` 区域指示符的单色字母字形。
//! 3. `CosmicTextSystemState::load_family` 会将所有缺少 ASCII `'m'` 映射的字体直接从
//!    `fontdb` 删除，且 `cosmic-text` 在中粗字重（500/600）下只匹配同字重或含 `wght`
//!    可变轴（`fvar`）的回退字体。
//!
//! 本模块在启动期于内存中为系统的 `NotoColorEmoji.ttf` 补齐 `'m'` 映射（ASCII 在 GPUI
//! 回退判定中固定走主字体，不会实际使用该字形）与 `wght` 可变字重轴（100..900），并解析出
//! 真实可用的 Linux UI 主字体与显式回退链。

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gpui::{App, FontFallbacks, FontWeight, SharedString, Styled, font};

/// Linux 桌面优先选用的 UI 主字体（按中日韩与西文覆盖质量排序）。
const PREFERRED_UI_FAMILIES: &[&str] = &[
    "Noto Sans CJK SC",
    "Noto Sans",
    "Ubuntu",
    "Cantarell",
    "DejaVu Sans",
];

/// Linux 显式字体回退链候选：将 `Noto Color Emoji` 置于 `FreeSans` 之前以拦截旗帜与 Emoji，
/// 随后补齐中日韩与通用西文回退。
const FALLBACK_CANDIDATES: &[&str] = &[
    "Noto Color Emoji",
    "Noto Sans CJK SC",
    "Noto Sans CJK TC",
    "WenQuanYi Micro Hei",
    "Noto Sans",
    "DejaVu Sans",
];

/// 常见发行版（Debian/Ubuntu、Arch、Fedora/RHEL、openSUSE 等）的 `NotoColorEmoji.ttf` 路径。
const NOTO_COLOR_EMOJI_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/NotoColorEmoji.ttf",
    "/usr/local/share/fonts/NotoColorEmoji.ttf",
];

#[derive(Clone, Debug)]
struct LinuxUiFontConfig {
    family: Option<SharedString>,
    fallbacks: Option<FontFallbacks>,
}

static UI_FONT_CONFIG: OnceLock<LinuxUiFontConfig> = OnceLock::new();

/// 启动期向 GPUI 注册修补后的 `Noto Color Emoji` 并解析 Linux 主界面字体与回退链。
pub(crate) fn configure_fonts(cx: &mut App) {
    let _ = UI_FONT_CONFIG.get_or_init(|| init_linux_fonts(cx));
}

/// 为根容器或独立浮层设置 Linux 主字体与 Emoji/CJK 回退链，避免退回缺失的 `.SystemUIFont`。
pub(crate) fn apply_ui_font<T: Styled>(mut element: T) -> T {
    if let Some(config) = UI_FONT_CONFIG.get() {
        let style = element.text_style();
        if let Some(family) = &config.family {
            style.font_family = Some(family.clone());
        }
        if let Some(fallbacks) = &config.fallbacks {
            style.font_fallbacks = Some(fallbacks.clone());
        }
    }
    element
}

fn init_linux_fonts(cx: &mut App) -> LinuxUiFontConfig {
    let text_system = cx.text_system();

    if let Some(path) = find_noto_color_emoji_path() {
        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Some(patched) = patch_noto_color_emoji_ttf(&bytes) {
                    if let Err(error) = text_system.add_fonts(vec![Cow::Owned(patched)]) {
                        log_warn!("app", "注册 Linux 彩色 Emoji 字体失败：{error:#}");
                    } else {
                        // 立即触发一次加载，让 GPUI 剔除系统中未含 'm' 映射的原始 face，
                        // 在 fontdb 中仅保留修补后的 Noto Color Emoji。
                        let _ = text_system.resolve_font(&font("Noto Color Emoji"));
                    }
                }
            }
            Err(error) => {
                log_warn!(
                    "app",
                    "读取系统 Emoji 字体 {} 失败：{error}",
                    path.display()
                );
            }
        }
    }

    let installed = text_system.all_font_names();
    let primary_family = select_primary_ui_family(&installed).map(SharedString::from);
    let fallback_list = build_fallback_list(&installed, primary_family.as_deref());
    let fallbacks = (!fallback_list.is_empty()).then(|| FontFallbacks::from_fonts(fallback_list));

    // 预热主字体常用字重（400/500/600），确保用户回退链在首次渲染前完成绑定。
    if let Some(primary) = &primary_family {
        for weight in [FontWeight::NORMAL, FontWeight::MEDIUM, FontWeight::SEMIBOLD] {
            let mut f = font(primary.clone());
            f.weight = weight;
            f.fallbacks = fallbacks.clone();
            let _ = text_system.resolve_font(&f);
        }
    }

    log_info!(
        "app",
        "Linux UI 字体已配置：主字体={}，回退链={}",
        primary_family.as_deref().unwrap_or(".SystemUIFont"),
        fallbacks
            .as_ref()
            .map(|fb| fb.fallback_list().join(", "))
            .unwrap_or_default()
    );

    LinuxUiFontConfig {
        family: primary_family,
        fallbacks,
    }
}

fn select_primary_ui_family(installed: &[String]) -> Option<&'static str> {
    PREFERRED_UI_FAMILIES
        .iter()
        .copied()
        .find(|candidate| installed.iter().any(|name| name == candidate))
}

fn build_fallback_list(installed: &[String], primary: Option<&str>) -> Vec<String> {
    FALLBACK_CANDIDATES
        .iter()
        .copied()
        .filter(|&candidate| {
            Some(candidate) != primary && installed.iter().any(|name| name == candidate)
        })
        .map(str::to_owned)
        .collect()
}

fn find_noto_color_emoji_path() -> Option<PathBuf> {
    for candidate in NOTO_COLOR_EMOJI_PATHS {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    find_font_file_in_dir(Path::new("/usr/share/fonts"), "NotoColorEmoji.ttf", 3)
}

fn find_font_file_in_dir(dir: &Path, target_name: &str, max_depth: usize) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(target_name) {
                return Some(path);
            }
        } else if path.is_dir()
            && let Some(found) = find_font_file_in_dir(&path, target_name, max_depth - 1)
        {
            return Some(found);
        }
    }
    None
}

#[derive(Clone, Copy, Debug)]
struct TableRecord {
    tag: [u8; 4],
    checksum: u32,
    offset: u32,
    length: u32,
}

/// 在内存中为 `NotoColorEmoji.ttf` 补齐：
/// 1. `cmap` Format 12 的 `'m'` (`0x6d`) 映射，避免 `CosmicTextSystemState::load_family` 将其从
///    `fontdb` 删除；
/// 2. 36 字节的 `fvar` 表（`wght` 轴 100..900），让 `cosmic-text` 在 Medium (500) / Semibold (600)
///    等非 400 字重下依然选中 `Noto Color Emoji` 而非退回 `FreeSans`。
fn patch_noto_color_emoji_ttf(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let sfnt_version = u32::from_be_bytes(data[0..4].try_into().ok()?);
    if sfnt_version != 0x0001_0000 && sfnt_version != 0x4F54_544F {
        return None;
    }
    let num_tables = u16::from_be_bytes(data[4..6].try_into().ok()?) as usize;
    if num_tables == 0 || num_tables > 64 || data.len() < 12 + num_tables * 16 {
        return None;
    }

    let mut records = Vec::with_capacity(num_tables + 1);
    for i in 0..num_tables {
        let base = 12 + i * 16;
        let tag: [u8; 4] = data[base..base + 4].try_into().ok()?;
        let checksum = u32::from_be_bytes(data[base + 4..base + 8].try_into().ok()?);
        let offset = u32::from_be_bytes(data[base + 8..base + 12].try_into().ok()?);
        let length = u32::from_be_bytes(data[base + 12..base + 16].try_into().ok()?);
        let end = (offset as usize).checked_add(length as usize)?;
        if end > data.len() {
            return None;
        }
        records.push(TableRecord {
            tag,
            checksum,
            offset,
            length,
        });
    }

    let cmap_idx = records.iter().position(|r| &r.tag == b"cmap")?;
    let cmap_rec = records[cmap_idx];
    let cmap_slice = &data[cmap_rec.offset as usize..(cmap_rec.offset + cmap_rec.length) as usize];
    let patched_cmap = patch_cmap_format12_with_ascii_m(cmap_slice)?;

    let mut out = data.to_vec();
    let new_cmap_offset = append_aligned(&mut out, &patched_cmap)?;
    records[cmap_idx].offset = new_cmap_offset;
    records[cmap_idx].length = patched_cmap.len() as u32;
    records[cmap_idx].checksum = calc_opentype_checksum(&patched_cmap);

    if !records.iter().any(|r| &r.tag == b"fvar") {
        let fvar_bytes = build_wght_fvar_table();
        let fvar_offset = append_aligned(&mut out, &fvar_bytes)?;
        records.push(TableRecord {
            tag: *b"fvar",
            checksum: calc_opentype_checksum(&fvar_bytes),
            offset: fvar_offset,
            length: fvar_bytes.len() as u32,
        });
    }

    let new_dir_end = 12 + records.len() * 16;
    // 新增一条 TableRecord 会使目录区向后延伸 16 字节；若原有小表（如 54 字节的 head）
    // 紧邻旧目录区存放，将其搬移到文件末尾即可避免移动 10MB 的 CBDT 位图表。
    for record in &mut records {
        if (record.offset as usize) < new_dir_end {
            let start = record.offset as usize;
            let end = start + record.length as usize;
            let table_copy = data[start..end].to_vec();
            record.offset = append_aligned(&mut out, &table_copy)?;
        }
    }

    records.sort_by_key(|r| r.tag);

    let new_num_tables = records.len() as u16;
    let highest_power_of_2 = 1u16 << new_num_tables.ilog2();
    let search_range = highest_power_of_2 * 16;
    let entry_selector = new_num_tables.ilog2() as u16;
    let range_shift = new_num_tables * 16 - search_range;

    out[4..6].copy_from_slice(&new_num_tables.to_be_bytes());
    out[6..8].copy_from_slice(&search_range.to_be_bytes());
    out[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    out[10..12].copy_from_slice(&range_shift.to_be_bytes());

    for (i, record) in records.iter().enumerate() {
        let base = 12 + i * 16;
        out[base..base + 4].copy_from_slice(&record.tag);
        out[base + 4..base + 8].copy_from_slice(&record.checksum.to_be_bytes());
        out[base + 8..base + 12].copy_from_slice(&record.offset.to_be_bytes());
        out[base + 12..base + 16].copy_from_slice(&record.length.to_be_bytes());
    }

    Some(out)
}

fn patch_cmap_format12_with_ascii_m(cmap: &[u8]) -> Option<Vec<u8>> {
    if cmap.len() < 4 {
        return None;
    }
    let num_enc = u16::from_be_bytes(cmap[2..4].try_into().ok()?) as usize;
    if cmap.len() < 4 + num_enc * 8 {
        return None;
    }

    let mut fmt12_sub_off = None;
    for i in 0..num_enc {
        let base = 4 + i * 8;
        let sub_off = u32::from_be_bytes(cmap[base + 4..base + 8].try_into().ok()?) as usize;
        if sub_off + 16 <= cmap.len() {
            let fmt = u16::from_be_bytes(cmap[sub_off..sub_off + 2].try_into().ok()?);
            if fmt == 12 {
                fmt12_sub_off = Some(sub_off);
                break;
            }
        }
    }
    let sub_off = fmt12_sub_off?;
    let sub_len = u32::from_be_bytes(cmap[sub_off + 4..sub_off + 8].try_into().ok()?) as usize;
    let num_groups = u32::from_be_bytes(cmap[sub_off + 12..sub_off + 16].try_into().ok()?) as usize;
    let groups_end = sub_off
        .checked_add(16)?
        .checked_add(num_groups.checked_mul(12)?)?;
    if sub_len != 16 + num_groups * 12 || groups_end > cmap.len() {
        return None;
    }

    let mut space_glyph_id = 1u32;
    let mut insert_idx = num_groups;
    for g in 0..num_groups {
        let goff = sub_off + 16 + g * 12;
        let start_char = u32::from_be_bytes(cmap[goff..goff + 4].try_into().ok()?);
        let end_char = u32::from_be_bytes(cmap[goff + 4..goff + 8].try_into().ok()?);
        let start_glyph = u32::from_be_bytes(cmap[goff + 8..goff + 12].try_into().ok()?);
        if start_char <= 0x20 && 0x20 <= end_char {
            space_glyph_id = start_glyph + (0x20 - start_char);
        }
        if start_char <= 0x6d && 0x6d <= end_char {
            return Some(cmap.to_vec());
        }
        if insert_idx == num_groups && start_char > 0x6d {
            insert_idx = g;
        }
    }

    let insert_pos = sub_off + 16 + insert_idx * 12;
    let mut patched = Vec::with_capacity(cmap.len() + 12);
    patched.extend_from_slice(&cmap[..insert_pos]);
    patched.extend_from_slice(&0x6du32.to_be_bytes());
    patched.extend_from_slice(&0x6du32.to_be_bytes());
    patched.extend_from_slice(&space_glyph_id.to_be_bytes());
    patched.extend_from_slice(&cmap[insert_pos..]);

    let new_sub_len = (sub_len + 12) as u32;
    let new_num_groups = (num_groups + 1) as u32;
    patched[sub_off + 4..sub_off + 8].copy_from_slice(&new_sub_len.to_be_bytes());
    patched[sub_off + 12..sub_off + 16].copy_from_slice(&new_num_groups.to_be_bytes());

    for i in 0..num_enc {
        let base = 4 + i * 8;
        let old_off = u32::from_be_bytes(patched[base + 4..base + 8].try_into().ok()?);
        if (old_off as usize) > sub_off {
            let updated = old_off.checked_add(12)?;
            patched[base + 4..base + 8].copy_from_slice(&updated.to_be_bytes());
        }
    }

    Some(patched)
}

/// 生成包含单个 `wght` 轴（100.0..=900.0，默认 400.0）的最小 OpenType `fvar` 表（36 字节）。
fn build_wght_fvar_table() -> [u8; 36] {
    let mut buf = [0u8; 36];
    buf[0..2].copy_from_slice(&1u16.to_be_bytes()); // majorVersion = 1
    buf[2..4].copy_from_slice(&0u16.to_be_bytes()); // minorVersion = 0
    buf[4..6].copy_from_slice(&16u16.to_be_bytes()); // axesArrayOffset = 16
    buf[6..8].copy_from_slice(&2u16.to_be_bytes()); // reserved = 2
    buf[8..10].copy_from_slice(&1u16.to_be_bytes()); // axisCount = 1
    buf[10..12].copy_from_slice(&20u16.to_be_bytes()); // axisSize = 20
    buf[12..14].copy_from_slice(&0u16.to_be_bytes()); // instanceCount = 0
    buf[14..16].copy_from_slice(&4u16.to_be_bytes()); // instanceSize = 4
    // VariationAxisRecord (20 bytes)
    buf[16..20].copy_from_slice(b"wght");
    buf[20..24].copy_from_slice(&(100u32 << 16).to_be_bytes()); // minValue = 100.0
    buf[24..28].copy_from_slice(&(400u32 << 16).to_be_bytes()); // defaultValue = 400.0
    buf[28..32].copy_from_slice(&(900u32 << 16).to_be_bytes()); // maxValue = 900.0
    buf[32..34].copy_from_slice(&0u16.to_be_bytes()); // flags = 0
    buf[34..36].copy_from_slice(&2u16.to_be_bytes()); // axisNameID = 2 (Subfamily)
    buf
}

fn append_aligned(out: &mut Vec<u8>, bytes: &[u8]) -> Option<u32> {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    let offset = u32::try_from(out.len()).ok()?;
    out.extend_from_slice(bytes);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    Some(offset)
}

fn calc_opentype_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut chunks = data.chunks_exact(4);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut last = [0u8; 4];
        last[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(last));
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_and_fallback_selection_respects_priority_and_deduplicates() {
        let installed = vec![
            "DejaVu Sans".to_string(),
            "Noto Color Emoji".to_string(),
            "Noto Sans CJK SC".to_string(),
            "Ubuntu".to_string(),
        ];
        let primary = select_primary_ui_family(&installed);
        assert_eq!(primary, Some("Noto Sans CJK SC"));

        let fallbacks = build_fallback_list(&installed, primary);
        assert_eq!(
            fallbacks,
            vec!["Noto Color Emoji".to_string(), "DejaVu Sans".to_string()]
        );
    }

    #[test]
    fn cmap_format12_patch_inserts_ascii_m_in_sorted_order() {
        // 构造含两个分段组 (0x20..0x20 -> 3, 0xa9..0xa9 -> 16) 的最小 Format 12 cmap 表。
        let mut cmap = Vec::new();
        cmap.extend_from_slice(&0u16.to_be_bytes()); // version
        cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables = 1
        cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID = 3
        cmap.extend_from_slice(&10u16.to_be_bytes()); // encodingID = 10
        cmap.extend_from_slice(&12u32.to_be_bytes()); // subtableOffset = 12
        // Format 12 header (16 bytes) + 2 groups (24 bytes) = 40 bytes
        cmap.extend_from_slice(&12u16.to_be_bytes()); // format = 12
        cmap.extend_from_slice(&0u16.to_be_bytes()); // reserved
        cmap.extend_from_slice(&40u32.to_be_bytes()); // length = 40
        cmap.extend_from_slice(&0u32.to_be_bytes()); // language
        cmap.extend_from_slice(&2u32.to_be_bytes()); // numGroups = 2
        // group 0: 0x20..0x20 -> glyph 3
        cmap.extend_from_slice(&0x20u32.to_be_bytes());
        cmap.extend_from_slice(&0x20u32.to_be_bytes());
        cmap.extend_from_slice(&3u32.to_be_bytes());
        // group 1: 0xa9..0xa9 -> glyph 16
        cmap.extend_from_slice(&0xa9u32.to_be_bytes());
        cmap.extend_from_slice(&0xa9u32.to_be_bytes());
        cmap.extend_from_slice(&16u32.to_be_bytes());

        let patched = patch_cmap_format12_with_ascii_m(&cmap).expect("cmap patch should succeed");
        assert_eq!(patched.len(), cmap.len() + 12);
        let num_groups = u32::from_be_bytes(patched[24..28].try_into().unwrap());
        assert_eq!(num_groups, 3);
        // 居中插入的 group 1 应为 0x6d..0x6d -> glyph 3
        let g1_start = u32::from_be_bytes(patched[40..44].try_into().unwrap());
        let g1_end = u32::from_be_bytes(patched[44..48].try_into().unwrap());
        let g1_glyph = u32::from_be_bytes(patched[48..52].try_into().unwrap());
        assert_eq!((g1_start, g1_end, g1_glyph), (0x6d, 0x6d, 3));
    }

    #[test]
    fn system_noto_color_emoji_shapes_regional_indicator_flags_as_emoji_across_weights() {
        let Some(path) = find_noto_color_emoji_path() else {
            return;
        };
        let raw = std::fs::read(&path).expect("read NotoColorEmoji.ttf");
        let patched = patch_noto_color_emoji_ttf(&raw).expect("patch NotoColorEmoji.ttf");

        let text_system = gpui_platform::current_platform(true).text_system();
        text_system
            .add_fonts(vec![Cow::Owned(patched)])
            .expect("add patched Noto Color Emoji");

        let all_names = text_system.all_font_names();
        let primary = select_primary_ui_family(&all_names).unwrap_or("DejaVu Sans");
        let fallback_list = build_fallback_list(&all_names, Some(primary));
        let fallbacks = FontFallbacks::from_fonts(fallback_list);

        // 先加载一次 Noto Color Emoji，淘汰系统未修补的同名字体项。
        let emoji_font_id = text_system
            .font_id(&font("Noto Color Emoji"))
            .expect("Noto Color Emoji should load without being removed");

        for weight in [FontWeight::NORMAL, FontWeight::MEDIUM, FontWeight::SEMIBOLD] {
            let mut ui_font = font(primary);
            ui_font.weight = weight;
            ui_font.fallbacks = Some(fallbacks.clone());

            let text = "🇭🇰 香港 01";
            let shaped = text_system.layout_line(
                text,
                gpui::px(14.0),
                &[gpui::FontRun {
                    len: text.len(),
                    font_id: text_system
                        .font_id(&ui_font)
                        .expect("resolve primary UI font"),
                }],
            );
            assert!(
                !shaped.runs.is_empty() && !shaped.runs[0].glyphs.is_empty(),
                "shaped runs should not be empty for weight {weight:?}"
            );
            let first_run = &shaped.runs[0];
            assert_eq!(
                first_run.font_id, emoji_font_id,
                "flag run at weight {weight:?} should use Noto Color Emoji"
            );
            // 两个区域指示符（\u{1F1ED}\u{1F1F0}）经 GSUB 合成单个旗帜字形，不能拆成 H/K 两个字形。
            assert_eq!(
                first_run.glyphs.len(),
                1,
                "regional indicator pair should ligate into 1 flag glyph at weight {weight:?}"
            );
            let first_glyph = &first_run.glyphs[0];
            assert!(
                first_glyph.is_emoji,
                "flag glyph at weight {weight:?} should be marked as color emoji"
            );
            let bounds = text_system
                .glyph_raster_bounds(&gpui::RenderGlyphParams {
                    font_id: first_run.font_id,
                    glyph_id: first_glyph.id,
                    font_size: gpui::px(14.0),
                    subpixel_variant: gpui::Point::default(),
                    dilation: 0,
                    scale_factor: 1.0,
                    is_emoji: first_glyph.is_emoji,
                    subpixel_rendering: false,
                })
                .expect("rasterize flag emoji glyph");
            assert!(
                bounds.size.width.0 > 0 && bounds.size.height.0 > 0,
                "rasterized flag bitmap should have non-zero size, got {:?}",
                bounds.size
            );
        }
    }
}
