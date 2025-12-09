use eframe::egui;

/// 设置多语言字体
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // 添加Noto Sans字体（支持中文、日文等）
    // 这里使用系统字体或嵌入的字体数据
    
    // macOS系统中文字体
    #[cfg(target_os = "macos")]
    {
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
            fonts.font_data.insert(
                "pingfang".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            // 将字体添加到各个字体族
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "pingfang".to_owned());
            
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("pingfang".to_owned());
        }
        
        // 日文字体
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc") {
            fonts.font_data.insert(
                "hiragino".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("hiragino".to_owned());
        }
    }
    
    // Linux系统字体
    #[cfg(target_os = "linux")]
    {
        // Noto Sans CJK
        let font_paths = vec![
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ];
        
        for path in font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "noto_sans_cjk".to_owned(),
                    egui::FontData::from_owned(font_data),
                );
                
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "noto_sans_cjk".to_owned());
                
                break;
            }
        }
    }
    
    // Windows系统字体
    #[cfg(target_os = "windows")]
    {
        // 微软雅黑
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts.font_data.insert(
                "microsoft_yahei".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "microsoft_yahei".to_owned());
        }
        
        // 日文字体
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msgothic.ttc") {
            fonts.font_data.insert(
                "ms_gothic".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("ms_gothic".to_owned());
        }
    }
    
    // 设置字体
    ctx.set_fonts(fonts);
}

/// 尝试加载Google Noto Sans字体（备用方案）
#[allow(dead_code)]
pub fn try_load_noto_fonts(_fonts: &mut egui::FontDefinitions) {
    // 这里可以添加从Google Fonts下载字体的逻辑
    // 或者将字体文件嵌入到二进制中
    
    // 示例：嵌入的字体数据（需要先下载字体文件）
    // const NOTO_SANS_SC: &[u8] = include_bytes!("../fonts/NotoSansSC-Regular.otf");
    // const NOTO_SANS_JP: &[u8] = include_bytes!("../fonts/NotoSansJP-Regular.otf");
    
    // fonts.font_data.insert(
    //     "noto_sans_sc".to_owned(),
    //     egui::FontData::from_static(NOTO_SANS_SC),
    // );
}

