//! 传输加密拦截缝（规格 4.6 节）
//!
//! 对上传/下载字节流的加/解密拦截接口。MVP 注入空实现
//! （[`PassthroughCipher`] 直通）；未来接入端到端加密
//! （X25519 密钥协商 + AES-GCM 流式加密）时，只需在挂载时注入
//! 真实的 [`TransportCipher`] 实现，文件服务传输主流程不变。

/// 传输层加密拦截器
///
/// 每个挂载点持有一个实例（`Arc<dyn TransportCipher>`）：
/// - 上传方向：网络字节经 [`decrypt_chunk`](Self::decrypt_chunk) 后落盘
/// - 下载方向：文件字节经 [`encrypt_chunk`](Self::encrypt_chunk) 后发送
///
/// 实现必须是无状态或内部同步的（chunk 按序独立处理），
/// 以便流式逐块调用
pub trait TransportCipher: Send + Sync {
    /// 下载方向：文件字节 → 网络字节
    fn encrypt_chunk(&self, data: Vec<u8>) -> Vec<u8>;

    /// 上传方向：网络字节 → 文件字节
    fn decrypt_chunk(&self, data: Vec<u8>) -> Vec<u8>;
}

/// 直通实现（MVP：明文传输，不做任何变换）
///
/// 设置区需常驻"明文传输、仅限可信内网"告知文案（规格 8 节）
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughCipher;

impl TransportCipher for PassthroughCipher {
    fn encrypt_chunk(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    fn decrypt_chunk(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passthrough_returns_data_unchanged() {
        let cipher = PassthroughCipher;
        let data = vec![1u8, 2, 3, 4];
        assert_eq!(cipher.encrypt_chunk(data.clone()), data);
        assert_eq!(cipher.decrypt_chunk(data.clone()), data);
    }
}
