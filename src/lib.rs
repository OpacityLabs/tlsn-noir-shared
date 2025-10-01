use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use bb_service_rs::ProofData;

#[derive(Debug, Clone, Copy)]
pub enum Role {
    Prover,
    Verifier,
}

#[derive(Error, Debug)]
pub enum PVChannelError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid socket address: {0}")]
    InvalidAddress(#[from] std::net::AddrParseError),
    #[error("Connection failed")]
    ConnectionFailed,
    #[error("No connection established")]
    NoConnection,
}

pub struct PVChannel {
    role: Role,
    stream: Option<TcpStream>,
    listener: Option<TcpListener>,
}

impl PVChannel {
    pub async fn new(socket_addr: &str, role: Role) -> Result<PVChannel, PVChannelError> {
        let addr: SocketAddr = socket_addr.parse()?;

        match role {
            Role::Verifier => {
                let listener = TcpListener::bind(addr).await?;
                Ok(PVChannel {
                    role,
                    stream: None,
                    listener: Some(listener),
                })
            }
            Role::Prover => {
                let stream = loop {
                    match TcpStream::connect(&addr).await {
                        Ok(s) => break s,
                        Err(e) if e.kind() == tokio::io::ErrorKind::ConnectionRefused => {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                        Err(e) => return Err(e.into()),
                    }
                };
                Ok(PVChannel {
                    role,
                    stream: Some(stream),
                    listener: None,
                })
            }
        }
    }

    pub async fn accept_connection(&mut self) -> Result<(), PVChannelError> {
        match self.role {
            Role::Verifier => {},
            _ => return Err(PVChannelError::ConnectionFailed),
        }

        if let Some(listener) = &self.listener {
            let (stream, _) = listener.accept().await?;
            self.stream = Some(stream);
            Ok(())
        } else {
            Err(PVChannelError::NoConnection)
        }
    }

    pub async fn send_proof_data(&mut self, proof_data: &ProofData) -> Result<(), PVChannelError> {
        let stream = self.stream.as_mut().ok_or(PVChannelError::NoConnection)?;

        let serialized = serde_json::to_vec(proof_data)?;
        let length = serialized.len() as u32;

        stream.write_all(&length.to_be_bytes()).await?;
        stream.write_all(&serialized).await?;
        stream.flush().await?;

        Ok(())
    }

    pub async fn receive_proof_data(&mut self) -> Result<ProofData, PVChannelError> {
        let stream = self.stream.as_mut().ok_or(PVChannelError::NoConnection)?;

        let mut length_bytes = [0u8; 4];
        stream.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        let mut buffer = vec![0u8; length];
        stream.read_exact(&mut buffer).await?;

        let proof_data: ProofData = serde_json::from_slice(&buffer)?;
        Ok(proof_data)
    }
}