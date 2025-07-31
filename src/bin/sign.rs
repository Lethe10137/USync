use base64::{engine::Engine, prelude::BASE64_URL_SAFE};
use clap::{Arg, Command};
use crc::{Crc, CRC_64_ECMA_182};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;

// CRC64校验器
const CRC64: Crc<u64> = Crc::<u64>::new(&CRC_64_ECMA_182);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PacketType {
    Data,
    Control,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub packet_type: PacketType,
    pub data: Vec<u8>,
    pub checksum: Option<u64>,    // CRC64 for data packets
    pub signature: Option<Vec<u8>>, // Ed25519 signature for non-data packets
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub private_key_b64: String,
    pub public_key_b64: String,
}

impl Packet {
    pub fn new_data_packet(data: Vec<u8>) -> Self {
        let checksum = CRC64.checksum(&data);
        Self {
            packet_type: PacketType::Data,
            data,
            checksum: Some(checksum),
            signature: None,
        }
    }

    pub fn new_signed_packet(packet_type: PacketType, data: Vec<u8>, signing_key: &SigningKey) -> Self {
        let signature = signing_key.sign(&data);
        Self {
            packet_type,
            data,
            checksum: None,
            signature: Some(signature.to_bytes().to_vec()),
        }
    }

    pub fn verify_data_packet(&self) -> Result<(), String> {
        match self.packet_type {
            PacketType::Data => {
                if let Some(checksum) = self.checksum {
                    let calculated_checksum = CRC64.checksum(&self.data);
                    if calculated_checksum == checksum {
                        Ok(())
                    } else {
                        Err(format!(
                            "CRC64 校验失败: 期望 {}, 实际 {}",
                            checksum, calculated_checksum
                        ))
                    }
                } else {
                    Err("数据包缺少CRC64校验码".to_string())
                }
            }
            _ => Err("不是数据包".to_string()),
        }
    }

    pub fn verify_signed_packet(&self, verifying_key: &VerifyingKey) -> Result<(), String> {
        match self.packet_type {
            PacketType::Data => Err("数据包不使用Ed25519签名验证".to_string()),
            _ => {
                if let Some(sig_bytes) = &self.signature {
                    let signature = Signature::from_bytes(
                        sig_bytes
                            .as_slice()
                            .try_into()
                            .map_err(|_| "签名长度错误".to_string())?,
                    );
                    verifying_key
                        .verify(&self.data, &signature)
                        .map_err(|e| format!("Ed25519签名验证失败: {}", e))
                } else {
                    Err("非数据包缺少Ed25519签名".to_string())
                }
            }
        }
    }
}

fn load_config_from_file(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&config_content)?;
    Ok(config)
}

fn load_keys_from_config(config: &Config) -> Result<(SigningKey, VerifyingKey), Box<dyn std::error::Error>> {
    let sk_bytes = BASE64_URL_SAFE.decode(&config.private_key_b64)?;
    let vk_bytes = BASE64_URL_SAFE.decode(&config.public_key_b64)?;

    let signing_key = SigningKey::from_bytes(&sk_bytes.try_into().map_err(|_| "私钥长度错误")?);
    let verifying_key = VerifyingKey::from_bytes(&vk_bytes.try_into().map_err(|_| "公钥长度错误")?)
        .map_err(|e| format!("公钥解析错误: {}", e))?;

    Ok((signing_key, verifying_key))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("签名验证工具")
        .version("1.0")
        .about("支持CRC64和Ed25519的数据包签名验证工具")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("配置文件路径 (TOML格式)")
                .required(false),
        )
        .arg(
            Arg::new("private-key")
                .short('s')
                .long("private-key")
                .value_name("BASE64")
                .help("Base64编码的私钥")
                .required(false),
        )
        .arg(
            Arg::new("public-key")
                .short('p')
                .long("public-key")
                .value_name("BASE64")
                .help("Base64编码的公钥")
                .required(false),
        )
        .get_matches();

    // 从配置文件或命令行参数加载密钥
    let (signing_key, verifying_key) = if let Some(config_path) = matches.get_one::<String>("config") {
        let config = load_config_from_file(config_path)?;
        load_keys_from_config(&config)?
    } else if let (Some(sk_b64), Some(vk_b64)) = (
        matches.get_one::<String>("private-key"),
        matches.get_one::<String>("public-key"),
    ) {
        let config = Config {
            private_key_b64: sk_b64.clone(),
            public_key_b64: vk_b64.clone(),
        };
        load_keys_from_config(&config)?
    } else {
        // 如果没有提供密钥，生成示例密钥用于演示
        println!("未提供密钥，生成示例密钥用于演示...");
        let randomness: [u8; 32] = [9; 32];
        let signing_key = SigningKey::from_bytes(&randomness);
        let verifying_key = signing_key.verifying_key();
        
        let sk_b64 = BASE64_URL_SAFE.encode(signing_key.to_bytes());
        let vk_b64 = BASE64_URL_SAFE.encode(verifying_key.to_bytes());
        println!("生成的私钥 (Base64): {}", sk_b64);
        println!("生成的公钥 (Base64): {}", vk_b64);
        
        (signing_key, verifying_key)
    };

    println!("✅ 密钥加载成功！");

    // 演示不同类型的数据包
    println!("\n=== 数据包处理演示 ===");

    // 1. 创建数据包 (使用CRC64)
    let data_payload = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let data_packet = Packet::new_data_packet(data_payload);
    println!("数据包: {:?}", data_packet);

    // 2. 验证数据包
    match data_packet.verify_data_packet() {
        Ok(()) => println!("✅ 数据包CRC64校验通过"),
        Err(e) => println!("❌ 数据包校验失败: {}", e),
    }

    // 3. 创建控制包 (使用Ed25519签名)
    let control_payload = b"CONTROL_COMMAND_START".to_vec();
    let control_packet = Packet::new_signed_packet(
        PacketType::Control,
        control_payload,
        &signing_key,
    );
    println!("控制包: {:?}", control_packet);

    // 4. 验证控制包
    match control_packet.verify_signed_packet(&verifying_key) {
        Ok(()) => println!("✅ 控制包Ed25519签名验证通过"),
        Err(e) => println!("❌ 控制包签名验证失败: {}", e),
    }

    // 5. 创建元数据包 (使用Ed25519签名)
    let metadata_payload = b"metadata_info_12345".to_vec();
    let metadata_packet = Packet::new_signed_packet(
        PacketType::Metadata,
        metadata_payload,
        &signing_key,
    );
    println!("元数据包: {:?}", metadata_packet);

    // 6. 验证元数据包
    match metadata_packet.verify_signed_packet(&verifying_key) {
        Ok(()) => println!("✅ 元数据包Ed25519签名验证通过"),
        Err(e) => println!("❌ 元数据包签名验证失败: {}", e),
    }

    // 7. 测试大数据包的CRC64性能
    println!("\n=== 大数据包性能测试 ===");
    let large_data = vec![42u8; 1_000_000]; // 1MB数据
    let start_time = std::time::Instant::now();
    let large_packet = Packet::new_data_packet(large_data);
    let create_time = start_time.elapsed();
    
    let verify_start = std::time::Instant::now();
    let verify_result = large_packet.verify_data_packet();
    let verify_time = verify_start.elapsed();
    
    println!("大数据包 (1MB) - 创建时间: {:?}, 验证时间: {:?}", create_time, verify_time);
    match verify_result {
        Ok(()) => println!("✅ 大数据包CRC64校验通过"),
        Err(e) => println!("❌ 大数据包校验失败: {}", e),
    }

    println!("\n=== 程序运行完成 ===");
    Ok(())
}