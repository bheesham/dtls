use super::flight0::*;
use super::flight4::*;
use super::*;
use crate::content::*;
use crate::errors::*;
use crate::handshake::handshake_message_hello_verify_request::*;
use crate::handshake::*;
use crate::record_layer::record_layer_header::*;

use util::Error;

use async_trait::async_trait;

pub(crate) struct Flight2;

#[async_trait]
impl Flight for Flight2 {
    fn to_string(&self) -> String {
        "Flight 2".to_owned()
    }

    fn has_retransmit(&self) -> bool {
        false
    }

    async fn parse(
        &self,
        tx: &mut mpsc::Sender<mpsc::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        let (seq, msgs) = match cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[HandshakeCachePullRule {
                    typ: HandshakeType::ClientHello,
                    epoch: cfg.initial_epoch,
                    is_client: true,
                    optional: false,
                }],
            )
            .await
        {
            // No valid message received. Keep reading
            Ok((seq, msgs)) => (seq, msgs),

            // Client may retransmit the first ClientHello when HelloVerifyRequest is dropped.
            // Parse as flight 0 in this case.
            Err(_) => return Flight0 {}.parse(tx, state, cache, cfg).await,
        };

        state.handshake_recv_sequence = seq;

        if let Some(message) = msgs.get(&HandshakeType::ClientHello) {
            // Validate type
            let client_hello = match message {
                HandshakeMessage::ClientHello(client_hello) => client_hello,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        None,
                    ))
                }
            };

            if client_hello.version != PROTOCOL_VERSION1_2 {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::ProtocolVersion,
                    }),
                    Some(ERR_UNSUPPORTED_PROTOCOL_VERSION.clone()),
                ));
            }

            if client_hello.cookie.is_empty() {
                return Err((None, None));
            }

            if state.cookie != client_hello.cookie {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::AccessDenied,
                    }),
                    Some(ERR_COOKIE_MISMATCH.clone()),
                ));
            }

            Ok(Box::new(Flight4 {}))
        } else {
            Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                None,
            ))
        }
    }

    async fn generate(
        &self,
        state: &mut State,
        _cache: &HandshakeCache,
        _cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
        state.handshake_send_sequence = 0;
        Ok(vec![Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::HelloVerifyRequest(
                    HandshakeMessageHelloVerifyRequest {
                        version: PROTOCOL_VERSION1_2,
                        cookie: state.cookie.clone(),
                    },
                ))),
            ),
            should_encrypt: false,
            reset_local_sequence_number: false,
        }])
    }
}
