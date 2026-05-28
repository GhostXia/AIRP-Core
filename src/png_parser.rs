use crate::error::AirpError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// M5.6：单个 PNG chunk 的最大允许字节数（16 MiB）。
/// 防御恶意 PNG 声明巨型 chunk 长度造成 OOM；正常角色卡 chara 块远低于此。
const MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// 从 PNG 文件中提取角色卡 JSON 数据。
/// 支持 `tEXt` 和 `iTXt` 文本块，键名为 `chara`。
pub fn parse_png_character_card<P: AsRef<Path>>(path: P) -> Result<String, AirpError> {
    let mut file = File::open(path)?;

    // 1. 验证 PNG 头部签名
    let mut signature = [0u8; 8];
    file.read_exact(&mut signature)?;
    if signature != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(AirpError::BadRequest("非法 PNG 文件签名".to_string()));
    }

    // 2. 循环读取 PNG Chunk
    loop {
        let mut length_buf = [0u8; 4];
        if file.read_exact(&mut length_buf).is_err() {
            // 到达文件末尾
            break;
        }
        let length = u32::from_be_bytes(length_buf) as usize;
        if length > MAX_CHUNK_SIZE {
            return Err(AirpError::BadRequest(format!(
                "PNG chunk 过大：{} 字节（上限 {} 字节）",
                length, MAX_CHUNK_SIZE
            )));
        }

        let mut type_buf = [0u8; 4];
        file.read_exact(&mut type_buf)?;
        let chunk_type = String::from_utf8_lossy(&type_buf).into_owned();

        // 读取 Chunk Data
        let mut data = vec![0u8; length];
        file.read_exact(&mut data)?;

        // 读取 CRC (跳过)
        let mut crc_buf = [0u8; 4];
        file.read_exact(&mut crc_buf)?;

        // 判断类型
        if chunk_type == "tEXt" {
            if let Some((keyword, text)) = parse_text_chunk(&data) {
                if keyword == "chara" {
                    return Ok(decode_chara_data(&text));
                }
            }
        } else if chunk_type == "iTXt" {
            if let Some((keyword, text)) = parse_itxt_chunk(&data) {
                if keyword == "chara" {
                    return Ok(decode_chara_data(&text));
                }
            }
        } else if chunk_type == "IEND" {
            break;
        }
    }

    Err(AirpError::BadRequest(
        "未在 PNG 文件中找到 chara 角色卡数据".to_string(),
    ))
}

fn parse_text_chunk(data: &[u8]) -> Option<(String, String)> {
    let null_pos = data.iter().position(|&b| b == 0)?;
    let keyword = String::from_utf8_lossy(&data[..null_pos]).into_owned();
    let text = String::from_utf8_lossy(&data[null_pos + 1..]).into_owned();
    Some((keyword, text))
}

fn parse_itxt_chunk(data: &[u8]) -> Option<(String, String)> {
    let k_null = data.iter().position(|&b| b == 0)?;
    let keyword = String::from_utf8_lossy(&data[..k_null]).into_owned();

    let idx = k_null + 1;
    if idx + 2 > data.len() {
        return None;
    }
    let compression_flag = data[idx];

    let lang_start = idx + 2;
    let lang_null = data[lang_start..].iter().position(|&b| b == 0)? + lang_start;

    let trans_start = lang_null + 1;
    let trans_null = data[trans_start..].iter().position(|&b| b == 0)? + trans_start;

    let text_bytes = &data[trans_null + 1..];

    if compression_flag == 1 {
        // 在 AIRP 生态中，基本不使用压缩的 iTXt 块存储 chara。
        // 如果有，这里会发出错误或跳过。
        return None;
    }

    let text = String::from_utf8_lossy(text_bytes).into_owned();
    Some((keyword, text))
}

fn decode_chara_data(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(decoded_bytes) = STANDARD.decode(trimmed) {
        if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
            return decoded_str;
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// 构造一个 PNG 头 + 单个 chunk 头（长度 + 类型）的字节流。
    /// `length` 写入 chunk 长度字段；不会真的填这么多字节（用于触发上限检查）。
    fn write_png_with_chunk_length(length: u32, chunk_type: &[u8; 4]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        // PNG 签名
        f.write_all(&[137, 80, 78, 71, 13, 10, 26, 10]).unwrap();
        // chunk length
        f.write_all(&length.to_be_bytes()).unwrap();
        // chunk type
        f.write_all(chunk_type).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_rejects_oversized_chunk() {
        // 17 MiB chunk → 应被上限拒绝，不分配 vec
        let f = write_png_with_chunk_length(17 * 1024 * 1024, b"tEXt");
        let res = parse_png_character_card(f.path());
        assert!(res.is_err(), "expected size-limit rejection, got {:?}", res);
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("过大") || msg.contains("上限"),
            "unexpected error msg: {}",
            msg
        );
    }

    #[test]
    fn test_rejects_bad_signature() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"NOTAPNGFILE").unwrap();
        f.flush().unwrap();
        assert!(parse_png_character_card(f.path()).is_err());
    }
}
