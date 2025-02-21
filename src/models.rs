use std::{io::Write, str::FromStr};

use anyhow::Context;

const NEWLINE: u8 = b'\n';
const NEWLINE_ARRAY: [u8; 1] = [NEWLINE];

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum Product {
    Apples,
    Pears,
    Tomatoes,
    Potatoes,
    Onions,
}

impl FromStr for Product {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "APPLE" => Ok(Self::Apples),
            "PEAR" => Ok(Self::Pears),
            "TOMATO" => Ok(Self::Tomatoes),
            "POTATO" => Ok(Self::Potatoes),
            "ONION" => Ok(Self::Onions),
            other => {
                anyhow::bail!("Unknown product: {other}");
            }
        }
    }
}

impl ToString for Product {
    fn to_string(&self) -> String {
        match self {
            Self::Apples => "APPLE".to_string(),
            Self::Pears => "PEAR".to_string(),
            Self::Tomatoes => "TOMATO".to_string(),
            Self::Potatoes => "POTATO".to_string(),
            Self::Onions => "ONION".to_string(),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

impl FromStr for Side {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            other => {
                anyhow::bail!("Unknown side: {other}");
            }
        }
    }
}

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

#[derive(Debug)]
pub struct Order {
    pub side: Side,
    pub product: Product,
}

impl FromStr for Order {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(':');
        let side = split.next().context("Message from client without side")?;
        let product = split
            .next()
            .context("Message from client without product")?;

        let side = side.parse()?;
        let product = product.parse()?;

        Ok(Self { side, product })
    }
}

#[derive(Debug)]
pub struct OrderAck {
    pub product: Product,
}

impl Encode for OrderAck {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        // ACK:{product}

        let mut length = 0;
        length += (&mut buffer[length..]).write(b"ACK:")?;
        length += (&mut buffer[length..]).write(self.product.to_string().as_bytes())?;
        length += (&mut buffer[length..]).write(&NEWLINE_ARRAY)?;

        tracing::debug!("OrderAck encoded: {:?}", &buffer[..length]);

        Ok(length)
    }
}

#[derive(Debug)]
pub struct Trade {
    pub product: Product,
}

impl Encode for Trade {
    fn encode(&self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        let mut length = 0;
        // TRADE:{product}
        length += (&mut buffer[length..]).write(b"TRADE:")?;
        length += (&mut buffer[length..]).write(self.product.to_string().as_bytes())?;
        length += (&mut buffer[length..]).write(&NEWLINE_ARRAY)?;

        tracing::debug!("Trade encoded: {:?}", &buffer[..length]);

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
