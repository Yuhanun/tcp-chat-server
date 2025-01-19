use std::io::Write;

const NEWLINE: u8 = b'\n';
const NEWLINE_ARRAY: [u8; 1] = [NEWLINE];

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct ClientId(pub u16);

#[derive(Debug)]
pub struct Login {
    pub client_id: ClientId,
}

pub trait Encode: Send + std::fmt::Debug {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize>;
}

impl Encode for Login {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        let mut length = 0;
        length += (&mut buffer[length..]).write(b"LOGIN:")?;
        length += (&mut buffer[length..]).write(self.client_id.0.to_string().as_bytes())?;
        length += (&mut buffer[length..]).write(&NEWLINE_ARRAY)?;

        tracing::debug!("Login encoded: {:?}", &buffer[..length]);

        Ok(length)
    }
}

#[derive(Debug)]
pub struct Message {
    pub origin_client_id: ClientId,
    pub message: String,
}

impl Encode for Message {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        let mut length = 0;
        length += (&mut buffer[length..]).write(b"MESSAGE:")?;
        length += (&mut buffer[length..]).write(self.origin_client_id.0.to_string().as_bytes())?;
        length += (&mut buffer[length..]).write(b" ")?;
        length += (&mut buffer[length..]).write(self.message.as_bytes())?;
        length += (&mut buffer[length..]).write(&NEWLINE_ARRAY)?;

        tracing::debug!("Message encoded: {:?}", &buffer[..length]);

        Ok(length)
    }
}

#[derive(Debug)]
pub struct MessageAck;

impl Encode for MessageAck {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        let length = (&mut buffer[0..]).write(b"ACK:MESSAGE\n")?;

        tracing::debug!("MessageAck encoded: {:?}", &buffer[..length]);

        Ok(length)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_login_encode() {
        let login = Login {
            client_id: ClientId(1),
        };

        let mut buffer = [0; 1024];
        let length = login.encode(&mut buffer).unwrap();

        assert_eq!(&buffer[..length], b"LOGIN:1\n");
    }

    #[test]
    fn test_message_encode() {
        let message = Message {
            origin_client_id: ClientId(1),
            message: "Hello, World!".to_string(),
        };

        let mut buffer = [0; 1024];
        let length = message.encode(&mut buffer).unwrap();

        assert_eq!(&buffer[..length], b"MESSAGE:1 Hello, World!\n");
    }

    #[test]
    fn test_message_ack_encode() {
        let message_ack = MessageAck;

        let mut buffer = [0; 1024];
        let length = message_ack.encode(&mut buffer).unwrap();

        assert_eq!(&buffer[..length], b"ACK:MESSAGE\n");
    }
}
