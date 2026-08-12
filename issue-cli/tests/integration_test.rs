//! 集成测试：创建含 A1111 元数据的 PNG 并验证解析

use std::fs::File;
use std::io::BufWriter;

/// 创建一个测试用 PNG 文件，包含 A1111 风格的参数元数据
fn create_test_png(path: &str, metadata: &str) {
    let file = File::create(path).expect("无法创建测试文件");
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, 64, 64); // 小图即可
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    encoder.add_text_chunk(
        "parameters".to_string(),
        metadata.to_string(),
    ).expect("添加文本块失败");

    let mut writer = encoder.write_header().expect("写入 PNG 头失败");
    let data = vec![255u8; 64 * 64 * 4]; // 纯白像素
    writer.write_image_data(&data).expect("写入图像数据失败");
    writer.finish().expect("完成 PNG 写入失败");
}

#[test]
fn test_integration_a1111_metadata() {
    let path = "/tmp/test_a1111_integration.png";
    let metadata = "masterpiece, best quality, cyberpunk girl, neon lights, intricate details\n\
Negative prompt: low quality, blurry, ugly, text, watermark\n\
Steps: 28, Sampler: DPM++ 2M Karras, CFG scale: 7, Seed: 123456789, Size: 1024x1024, \
Model hash: abc123def, Model: sd3.5_medium, VAE: vae-ft-mse-840000.safetensors";

    create_test_png(path, metadata);

    let meta = ai_gallery_cli::metadata::read_png_metadata(path)
        .expect("应该成功解析元数据");

    assert_eq!(meta.source, "A1111");
    assert_eq!(meta.prompt, "masterpiece, best quality, cyberpunk girl, neon lights, intricate details");
    assert_eq!(meta.negative, "low quality, blurry, ugly, text, watermark");
    assert_eq!(meta.seed, 123456789);
    assert_eq!(meta.model, "sd3.5_medium");
    assert_eq!(meta.model_hash, "abc123def");
    assert_eq!(meta.cfg_scale, 7.0);
    assert_eq!(meta.steps, 28);
    assert_eq!(meta.sampler, "DPM++ 2M Karras");
    assert_eq!(meta.width, 1024);
    assert_eq!(meta.height, 1024);

    std::fs::remove_file(path).ok();
}

#[test]
fn test_integration_comfyui_metadata() {
    let path = "/tmp/test_comfyui_integration.png";
    let file = File::create(path).expect("无法创建测试文件");
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    // ComfyUI workflow JSON
    let workflow = serde_json::json!({
        "3": {
            "class_type": "KSampler",
            "inputs": {
                "seed": 999999,
                "steps": 30,
                "cfg": 8.0,
                "sampler_name": "euler",
                "scheduler": "normal",
                "denoise": 1.0
            }
        },
        "4": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": "a beautiful landscape, detailed, high quality"
            }
        },
        "5": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": "bad quality, blurry, ugly"
            }
        },
        "6": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {
                "ckpt_name": "sd_xl_base_1.0.safetensors"
            }
        }
    });

    encoder.add_text_chunk(
        "prompt".to_string(),
        workflow.to_string(),
    ).expect("添加文本块失败");

    let mut writer = encoder.write_header().expect("写入 PNG 头失败");
    let data = vec![0u8; 64 * 64 * 4];
    writer.write_image_data(&data).expect("写入图像数据失败");
    writer.finish().expect("完成 PNG 写入失败");

    let meta = ai_gallery_cli::metadata::read_png_metadata(path)
        .expect("应该成功解析 ComfyUI 元数据");

    assert_eq!(meta.source, "ComfyUI");
    assert_eq!(meta.seed, 999999);
    assert_eq!(meta.steps, 30);
    // 使用接近比较而非精确相等
    assert!((meta.cfg_scale - 8.0).abs() < 0.001);
    assert!(meta.sampler.contains("euler"));
    assert!(meta.model.contains("sd_xl_base_1.0"));

    std::fs::remove_file(path).ok();
}