use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::error::Error;

const KEY_LEN: usize = 32;

fn get_encryption_key() -> Result<[u8; KEY_LEN], Box<dyn Error>> {
    let home = std::env::var("HOME")?;
    let key_file = format!("{}/.dioxus_music/encryption.key", home);

    let key: [u8; KEY_LEN] = if std::path::Path::new(&key_file).exists() {
        let key_data = std::fs::read(&key_file)?;
        if key_data.len() != KEY_LEN {
            return Err("Invalid encryption key file".into());
        }
        key_data.try_into().unwrap()
    } else {
        let mut key = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut key);
        std::fs::create_dir_all(format!("{}/.dioxus_music", home))?;
        std::fs::write(&key_file, &key)?;
        key
    };

    Ok(key)
}

fn derive_key_from_password(password: &str) -> Result<[u8; KEY_LEN], Box<dyn Error>> {
    let encryption_key = get_encryption_key()?;
    let mut hasher = Sha256::default();
    hasher.update(b"DioxusMusic_Password_Key");
    hasher.update(password.as_bytes());
    hasher.update(&encryption_key);
    let result = hasher.finalize();
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&result[..KEY_LEN]);
    Ok(key)
}

pub fn encrypt_password(password: &str, master_password: &str) -> Result<String, Box<dyn Error>> {
    eprintln!("[Crypto] 加密: password={}, master_len={}", password, master_password.len());
    
    let key = derive_key_from_password(master_password)?;
    eprintln!("[Crypto] key[0..8]={:02x?}", &key[..8]);
    
    let plaintext = password.as_bytes();
    let plaintext_len = plaintext.len();
    eprintln!("[Crypto] 明文长度={}", plaintext_len);
    
    let padded_len = if plaintext_len % 16 == 0 {
        plaintext_len
    } else {
        plaintext_len + (16 - plaintext_len % 16)
    };
    eprintln!("[Crypto] 填充后长度={}", padded_len);
    
    let mut padded_plaintext = vec![0u8; padded_len];
    padded_plaintext[..plaintext_len].copy_from_slice(plaintext);
    padded_plaintext[plaintext_len] = 0x80;
    
    let mut iv = [0u8; 16];
    OsRng.fill_bytes(&mut iv);
    eprintln!("[Crypto] iv[0..8]={:02x?}", &iv[..8]);
    
    let mut ciphertext = Vec::with_capacity(iv.len() + padded_len);
    ciphertext.extend_from_slice(&iv);
    
    let mut previous_block = iv;
    let mut block = [0u8; 16];
    
    for i in 0..(padded_len / 16) {
        let start = i * 16;
        
        for j in 0..16 {
            block[j] = padded_plaintext[start + j] ^ previous_block[j] ^ key[j];
        }
        
        ciphertext.extend_from_slice(&block);
        previous_block = block;
    }
    
    let result = BASE64.encode(&ciphertext);
    eprintln!("[Crypto] 加密完成: 结果长度={}", result.len());
    Ok(result)
}

pub fn decrypt_password(encrypted: &str, master_password: &str) -> Result<String, Box<dyn Error>> {
    eprintln!("[Crypto] 解密: 输入长度={}, master_len={}", encrypted.len(), master_password.len());
    
    let key = derive_key_from_password(master_password)?;
    eprintln!("[Crypto] key[0..8]={:02x?}", &key[..8]);
    
    let data = BASE64.decode(encrypted)?;
    eprintln!("[Crypto] base64解码后长度={}", data.len());
    
    if data.len() < 16 {
        return Err("Invalid encrypted data: too short".into());
    }
    
    let iv = &data[..16];
    let ciphertext = &data[16..];
    eprintln!("[Crypto] iv长度={}, 密文长度={}", iv.len(), ciphertext.len());
    eprintln!("[Crypto] iv[0..8]={:02x?}", &iv[..8]);
    eprintln!("[Crypto] ciphertext[0..8]={:02x?}", &ciphertext[..std::cmp::min(8, ciphertext.len())]);
    
    if ciphertext.len() % 16 != 0 {
        return Err("Invalid ciphertext length".into());
    }
    
    let mut plaintext = Vec::new();
    let mut previous_block = iv.to_vec();
    
    for chunk in ciphertext.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        
        for j in 0..16 {
            let decrypted_byte = block[j] ^ previous_block[j] ^ key[j];
            plaintext.push(decrypted_byte);
        }
        
        previous_block = block.to_vec();
    }
    
    eprintln!("[Crypto] 解密后原始数据长度={}, bytes={:02x?}", plaintext.len(), &plaintext[..std::cmp::min(16, plaintext.len())]);
    
    // Remove 0x80 followed by 0x00 padding
    // Data format: [original data][0x80][0x00][0x00]...
    let mut trim_count = 0;
    
    // Find the 0x80 from the end
    for (i, &byte) in plaintext.iter().enumerate().rev() {
        if byte == 0x80 {
            trim_count = plaintext.len() - i;
            break;
        }
    }
    
    eprintln!("[Crypto] 找到0x80位置, trim_count={}", trim_count);
    
    if trim_count > 0 {
        plaintext.truncate(plaintext.len() - trim_count);
    }
    
    eprintln!("[Crypto] 最终明文长度={}", plaintext.len());
    
    Ok(String::from_utf8(plaintext)?)
}

pub fn generate_master_password() -> String {
    let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*";
    let mut rng = OsRng {};
    let mut password = String::with_capacity(32);
    for _ in 0..32 {
        let idx = (rng.next_u32() as usize) % chars.len();
        password.push(chars.chars().nth(idx).unwrap());
    }
    password
}

pub fn get_master_password() -> Result<String, Box<dyn Error>> {
    let home = std::env::var("HOME")?;
    let master_file = format!("{}/.dioxus_music/.master", home);
    let config_dir = format!("{}/.dioxus_music", home);

    if std::path::Path::new(&master_file).exists() {
        Ok(std::fs::read_to_string(&master_file)?)
    } else {
        std::fs::create_dir_all(&config_dir)?;
        let password = generate_master_password();
        std::fs::write(&master_file, &password)?;
        Ok(password)
    }
}
