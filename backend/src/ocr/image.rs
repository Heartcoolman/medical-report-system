#[cfg(feature = "ocr")]
pub fn extract_image_text(file_path: &str) -> Result<String, String> {
    let mut lt =
        leptess::LepTess::new(None, "chi_sim").map_err(|e| format!("初始化OCR引擎失败: {}", e))?;
    lt.set_image(file_path)
        .map_err(|e| format!("加载图片失败: {}", e))?;
    let text = lt
        .get_utf8_text()
        .map_err(|e| format!("OCR识别失败: {}", e))?;
    Ok(text)
}

#[cfg(not(feature = "ocr"))]
pub fn extract_image_text(_file_path: &str) -> Result<String, String> {
    Err("图片OCR功能未启用，请安装 tesseract 和 leptonica 后使用 --features ocr 编译".to_string())
}
