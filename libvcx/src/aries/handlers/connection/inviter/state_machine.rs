use std::collections::HashMap;

use crate::api::VcxStateType;
use crate::aries::handlers::connection::agent_info::AgentInfo;
use crate::aries::handlers::connection::inviter::states::complete::CompleteState;
use crate::aries::handlers::connection::inviter::states::invited::InvitedState;
use crate::aries::handlers::connection::inviter::states::null::NullState;
use crate::aries::handlers::connection::inviter::states::responded::RespondedState;
use crate::aries::handlers::connection::messages::DidExchangeMessages;
use crate::aries::messages::a2a::A2AMessage;
use crate::aries::messages::a2a::protocol_registry::ProtocolRegistry;
use crate::aries::messages::connection::did_doc::DidDoc;
use crate::aries::messages::connection::invite::Invitation;
use crate::aries::messages::connection::problem_report::{ProblemCode, ProblemReport};
use crate::aries::messages::connection::request::Request;
use crate::aries::messages::discovery::disclose::{Disclose, ProtocolDescriptor};
use crate::aries::messages::trust_ping::ping::Ping;
use crate::aries::messages::trust_ping::ping_response::PingResponse;
use crate::error::prelude::*;
use crate::aries::messages::ack::Ack;
use crate::aries::messages::discovery::query::Query;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmConnectionInviter {
    source_id: String,
    agent_info: AgentInfo,
    state: InviterState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InviterState {
    Null(NullState),
    Invited(InvitedState),
    Responded(RespondedState),
    Completed(CompleteState),
}

impl InviterState {
    pub fn code(&self) -> u32 {
        match self {
            InviterState::Null(_) => VcxStateType::VcxStateInitialized as u32,
            InviterState::Invited(_) => VcxStateType::VcxStateOfferSent as u32,
            InviterState::Responded(_) => VcxStateType::VcxStateRequestReceived as u32,
            InviterState::Completed(_) => VcxStateType::VcxStateAccepted as u32,
        }
    }
}

impl SmConnectionInviter {
    pub fn _build_inviter(source_id: &str) -> Self {
        SmConnectionInviter {
            source_id: source_id.to_string(),
            state: InviterState::Null(NullState {}),
            agent_info: AgentInfo::default(),
        }
    }

    pub fn is_in_null_state(&self) -> bool {
        match self.state {
            InviterState::Null(_) => true,
            _ => false
        }
    }

    pub fn from(source_id: String, agent_info: AgentInfo, state: InviterState) -> Self {
        SmConnectionInviter {
            source_id,
            agent_info,
            state,
        }
    }

    pub fn agent_info(&self) -> &AgentInfo {
        &self.agent_info
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn state(&self) -> u32 {
        self.state.code()
    }

    pub fn state_object(&self) -> &InviterState {
        &self.state
    }

    pub fn step(self, message: DidExchangeMessages) -> VcxResult<SmConnectionInviter> {
        trace!("SmConnectionInviter::step >>> message: {:?}", message);
        let SmConnectionInviter { source_id, agent_info, state } = self;

        trace!("SmConnectionInviter::step :: current state = {:?}", &state);
        let (new_state, agent_info) =
            SmConnectionInviter::inviter_step(state, message, &source_id, agent_info)?;

        trace!("SmConnectionInviter::step :: new state = {:?}", &new_state);
        Ok(SmConnectionInviter { source_id, agent_info, state: new_state })
    }

    pub fn their_did_doc(&self) -> Option<DidDoc> {
        match self.state {
            InviterState::Null(_) => None,
            InviterState::Invited(ref _state) => None,
            InviterState::Responded(ref state) => Some(state.did_doc.clone()),
            InviterState::Completed(ref state) => Some(state.did_doc.clone()),
        }
    }

    pub fn get_invitation(&self) -> Option<&Invitation> {
        match self.state {
            InviterState::Invited(ref state) => Some(&state.invitation),
            _ => None
        }
    }

    pub fn find_message_to_handle(&self, messages: HashMap<String, A2AMessage>) -> Option<(String, A2AMessage)> {
        for (uid, message) in messages {
            if self.can_handle_message(&message) {
                return Some((uid, message));
            }
        }
        None
    }

    pub fn get_protocols(&self) -> Vec<ProtocolDescriptor> {
        ProtocolRegistry::init().protocols()
    }

    pub fn get_remote_protocols(&self) -> Option<Vec<ProtocolDescriptor>> {
        match self.state {
            InviterState::Completed(ref state) => state.protocols.clone(),
            _ => None
        }
    }

    pub fn remote_did(&self) -> VcxResult<String> {
        self.their_did_doc()
            .map(|did_doc: DidDoc| did_doc.id.clone())
            .ok_or(VcxError::from_msg(VcxErrorKind::NotReady, "Remote Connection DID is not set"))
    }

    pub fn remote_vk(&self) -> VcxResult<String> {
        self.their_did_doc()
            .and_then(|did_doc| did_doc.recipient_keys().get(0).cloned())
            .ok_or(VcxError::from_msg(VcxErrorKind::NotReady, "Remote Connection Verkey is not set"))
    }

    pub fn prev_agent_info(&self) -> Option<&AgentInfo> {
        match self.state {
            InviterState::Responded(ref state) => Some(&state.prev_agent_info),
            _ => None
        }
    }

    pub fn new(source_id: &str) -> Self {
        SmConnectionInviter::_build_inviter(source_id)
    }

    pub fn can_handle_message(&self, message: &A2AMessage) -> bool {
        match self.state {
            InviterState::Invited(_) => {
                match message {
                    A2AMessage::ConnectionRequest(_) => {
                        debug!("Inviter received ConnectionRequest message");
                        true
                    }
                    A2AMessage::ConnectionProblemReport(_) => {
                        debug!("Inviter received ProblemReport message");
                        true
                    }
                    _ => {
                        debug!("Inviter received unexpected message: {:?}", message);
                        false
                    }
                }
            }
            InviterState::Responded(_) => {
                match message {
                    A2AMessage::Ack(_) => {
                        debug!("Ack message received");
                        true
                    }
                    A2AMessage::Ping(_) => {
                        debug!("Ping message received");
                        true
                    }
                    A2AMessage::PingResponse(_) => {
                        debug!("PingResponse message received");
                        true
                    }
                    A2AMessage::ConnectionProblemReport(_) => {
                        debug!("ProblemReport message received");
                        true
                    }
                    _ => {
                        debug!("Unexpected message received in Responded state: {:?}", message);
                        false
                    }
                }
            }
            InviterState::Completed(_) => {
                match message {
                    A2AMessage::Ping(_) => {
                        debug!("Ping message received");
                        true
                    }
                    A2AMessage::PingResponse(_) => {
                        debug!("PingResponse message received");
                        true
                    }
                    A2AMessage::Query(_) => {
                        debug!("Query message received");
                        true
                    }
                    A2AMessage::Disclose(_) => {
                        debug!("Disclose message received");
                        true
                    }
                    _ => {
                        debug!("Unexpected message received in Completed state: {:?}", message);
                        false
                    }
                }
            }
            _ => {
                debug!("Unexpected message received: message: {:?}", message);
                false
            }
        }
    }

    pub fn transition_connect(inviter_state: InviterState, source_id: &str, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Null(state) => {
                agent_info = agent_info.create_agent()?;

                let invite: Invitation = Invitation::create()
                    .set_label(source_id.to_string())
                    .set_service_endpoint(agent_info.agency_endpoint()?)
                    .set_recipient_keys(agent_info.recipient_keys())
                    .set_routing_keys(agent_info.routing_keys()?);

                InviterState::Invited((state, invite).into())
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_receive_connection_request(inviter_state: InviterState, request: Request, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Invited(state) => {
                match state.handle_connection_request(&request, &agent_info) {
                    Ok((response, new_agent_info)) => {
                        let prev_agent_info = agent_info.clone();
                        agent_info = new_agent_info;
                        InviterState::Responded((state, request, response, prev_agent_info).into())
                    }
                    Err(err) => {
                        let problem_report = ProblemReport::create()
                            .set_problem_code(ProblemCode::RequestProcessingError)
                            .set_explain(err.to_string())
                            .set_thread_id(&request.id.0);

                        request.connection.did_doc.send_message(&problem_report.to_a2a_message(), &agent_info.pw_vk).ok();
                        InviterState::Null((state, problem_report).into())
                    }
                }
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_receive_ping(inviter_state: InviterState, ping: Ping, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Responded(state) => {
                state.handle_ping(&ping, &agent_info)?;
                InviterState::Completed((state, ping).into())
            }
            InviterState::Completed(state) => {
                state.handle_ping(&ping, &agent_info)?;
                InviterState::Completed(state)
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_send_ping(inviter_state: InviterState, comment: Option<String>, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Responded(state) => {
                let ping =
                    Ping::create()
                        .request_response()
                        .set_comment(comment);

                state.did_doc.send_message(&ping.to_a2a_message(), &agent_info.pw_vk).ok();
                InviterState::Responded(state)
            }
            InviterState::Completed(state) => {
                state.handle_send_ping(comment, &agent_info)?;
                InviterState::Completed(state)
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_ping_response_received(inviter_state: InviterState, ping_response: PingResponse, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Responded(state) => {
                InviterState::Completed((state, ping_response).into())
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_discover_features_received(inviter_state: InviterState, mut agent_info: AgentInfo, query_: Option<String>, comment: Option<String>) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Completed(state) => {
                state.handle_discover_features(query_, comment, &agent_info)?;
                InviterState::Completed(state)
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_discovery_query_received(inviter_state: InviterState, mut agent_info: AgentInfo, query: Query) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Completed(state) => {
                state.handle_discovery_query(query, &agent_info)?;
                InviterState::Completed(state)
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_disclose_received(inviter_state: InviterState, mut agent_info: AgentInfo, disclose: Disclose) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Completed(state) => {
                InviterState::Completed((state.clone(), disclose.protocols).into())
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_receive_problem_report(inviter_state: InviterState, mut agent_info: AgentInfo, problem_report: ProblemReport) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Responded(state) => {
                InviterState::Null((state, problem_report).into())
            }
            InviterState::Invited(state) => {
                InviterState::Null((state, problem_report).into())
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn transition_receive_ack(inviter_state: InviterState, mut agent_info: AgentInfo, ack: Ack) -> VcxResult<(InviterState, AgentInfo)> {
        let new_state = match inviter_state {
            InviterState::Responded(state) => {
                InviterState::Completed((state, ack).into())
            }
            _ => {
                inviter_state.clone()
            }
        };
        Ok((new_state, agent_info))
    }

    pub fn inviter_step(inviter_state: InviterState, message: DidExchangeMessages, source_id: &str, mut agent_info: AgentInfo) -> VcxResult<(InviterState, AgentInfo)> {
        match message {
            DidExchangeMessages::Connect() => {
                SmConnectionInviter::transition_connect(inviter_state, source_id, agent_info)
            }
            DidExchangeMessages::ExchangeRequestReceived(request) => {
                SmConnectionInviter::transition_receive_connection_request(inviter_state, request, agent_info)
            }
            DidExchangeMessages::AckReceived(ack) => {
                SmConnectionInviter::transition_receive_ack(inviter_state, agent_info, ack)
            }
            DidExchangeMessages::PingReceived(ping) => {
                SmConnectionInviter::transition_receive_ping(inviter_state, ping, agent_info)
            }
            DidExchangeMessages::ProblemReportReceived(problem_report) => {
                SmConnectionInviter::transition_receive_problem_report(inviter_state, agent_info, problem_report)
            }
            DidExchangeMessages::SendPing(comment) => {
                SmConnectionInviter::transition_send_ping(inviter_state, comment, agent_info)
            }
            DidExchangeMessages::PingResponseReceived(ping_response) => {
                SmConnectionInviter::transition_ping_response_received(inviter_state, ping_response, agent_info)
            }
            DidExchangeMessages::DiscoverFeatures((query_, comment)) => {
                SmConnectionInviter::transition_discover_features_received(inviter_state, agent_info, query_, comment)
            }
            DidExchangeMessages::QueryReceived(query) => {
                SmConnectionInviter::transition_discovery_query_received(inviter_state, agent_info, query)
            }
            DidExchangeMessages::DiscloseReceived(disclose) => {
                SmConnectionInviter::transition_disclose_received(inviter_state, agent_info, disclose)
            }
            DidExchangeMessages::InvitationReceived(_) => unimplemented!("Not valid for inviter"),
            DidExchangeMessages::ExchangeResponseReceived(_) => unimplemented!("Not valid for inviter"),
            DidExchangeMessages::Unknown => {
                Ok((inviter_state.clone(), agent_info))
            }
        }
    }
}


#[cfg(test)]
pub mod test {
    use crate::aries::messages::ack::tests::_ack;
    use crate::aries::messages::connection::problem_report::tests::_problem_report;
    use crate::aries::messages::connection::request::tests::_request;
    use crate::aries::messages::connection::response::tests::_signed_response;
    use crate::aries::messages::discovery::disclose::tests::_disclose;
    use crate::aries::messages::discovery::query::tests::_query;
    use crate::aries::messages::trust_ping::ping::tests::_ping;
    use crate::aries::messages::trust_ping::ping_response::tests::_ping_response;
    use crate::aries::test::source_id;
    use crate::utils::devsetup::SetupMocks;

    use super::*;

    pub mod inviter {
        use super::*;

        pub fn inviter_sm() -> SmConnectionInviter {
            SmConnectionInviter::new(&source_id())
        }

        impl SmConnectionInviter {
            fn to_inviter_invited_state(mut self) -> SmConnectionInviter {
                self = self.step(DidExchangeMessages::Connect()).unwrap();
                self
            }

            fn to_inviter_responded_state(mut self) -> SmConnectionInviter {
                self = self.step(DidExchangeMessages::Connect()).unwrap();
                self = self.step(DidExchangeMessages::ExchangeRequestReceived(_request())).unwrap();
                self
            }

            fn to_inviter_completed_state(mut self) -> SmConnectionInviter {
                self = self.step(DidExchangeMessages::Connect()).unwrap();
                self = self.step(DidExchangeMessages::ExchangeRequestReceived(_request())).unwrap();
                self = self.step(DidExchangeMessages::AckReceived(_ack())).unwrap();
                self
            }
        }

        mod new {
            use super::*;

            #[test]
            #[cfg(feature = "general_test")]
            fn test_inviter_new() {
                let _setup = SetupMocks::init();

                let inviter_sm = inviter_sm();

                assert_match!(InviterState::Null(_), inviter_sm.state);
                assert_eq!(source_id(), inviter_sm.source_id());
            }
        }

        mod step {
            use crate::utils::devsetup::SetupIndyMocks;

            use super::*;

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_init() {
                let _setup = SetupIndyMocks::init();

                let did_exchange_sm = inviter_sm();
                assert_match!(InviterState::Null(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_connect_message_from_null_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::Connect()).unwrap();

                assert_match!(InviterState::Invited(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_other_messages_from_null_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::AckReceived(_ack())).unwrap();
                assert_match!(InviterState::Null(_), did_exchange_sm.state);

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ProblemReportReceived(_problem_report())).unwrap();
                assert_match!(InviterState::Null(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_exchange_request_message_from_invited_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_invited_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ExchangeRequestReceived(_request())).unwrap();
                assert_match!(InviterState::Responded(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_invalid_exchange_request_message_from_invited_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_invited_state();

                let mut request = _request();
                request.connection.did_doc = DidDoc::default();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ExchangeRequestReceived(request)).unwrap();

                assert_match!(InviterState::Null(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_problem_report_message_from_invited_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_invited_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ProblemReportReceived(_problem_report())).unwrap();

                assert_match!(InviterState::Null(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_other_messages_from_invited_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_invited_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::Connect()).unwrap();
                assert_match!(InviterState::Invited(_), did_exchange_sm.state);

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::AckReceived(_ack())).unwrap();
                assert_match!(InviterState::Invited(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_ack_message_from_responded_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_responded_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::AckReceived(_ack())).unwrap();


                assert_match!(InviterState::Completed(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_ping_message_from_responded_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_responded_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::PingReceived(_ping())).unwrap();

                assert_match!(InviterState::Completed(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_problem_report_message_from_responded_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_responded_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ProblemReportReceived(_problem_report())).unwrap();

                assert_match!(InviterState::Null(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_other_messages_from_responded_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_responded_state();

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::Connect()).unwrap();

                assert_match!(InviterState::Responded(_), did_exchange_sm.state);
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_did_exchange_handle_messages_from_completed_state() {
                let _setup = SetupIndyMocks::init();

                let mut did_exchange_sm = inviter_sm().to_inviter_completed_state();

                // Send Ping
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::SendPing(None)).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Ping
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::PingReceived(_ping())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Ping Response
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::PingResponseReceived(_ping_response())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Discovery Features
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::DiscoverFeatures((None, None))).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Query
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::QueryReceived(_query())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Disclose
                assert!(did_exchange_sm.get_remote_protocols().is_none());

                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::DiscloseReceived(_disclose())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                assert!(did_exchange_sm.get_remote_protocols().is_some());

                // ignore
                // Ack
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::AckReceived(_ack())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);

                // Problem Report
                did_exchange_sm = did_exchange_sm.step(DidExchangeMessages::ProblemReportReceived(_problem_report())).unwrap();
                assert_match!(InviterState::Completed(_), did_exchange_sm.state);
            }
        }

        mod find_message_to_handle {
            use crate::utils::devsetup::SetupIndyMocks;

            use super::*;

            #[test]
            #[cfg(feature = "general_test")]
            fn test_find_message_to_handle_from_null_state() {
                let _setup = SetupIndyMocks::init();

                let connection = inviter_sm();

                // No messages
                {
                    let messages = map!(
                    "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                    "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response()),
                    "key_3".to_string() => A2AMessage::ConnectionProblemReport(_problem_report()),
                    "key_4".to_string() => A2AMessage::Ping(_ping()),
                    "key_5".to_string() => A2AMessage::Ack(_ack())
                );

                    assert!(connection.find_message_to_handle(messages).is_none());
                }
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_find_message_to_handle_from_invited_state() {
                let _setup = SetupIndyMocks::init();

                let connection = inviter_sm().to_inviter_invited_state();

                // Connection Request
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::Ping(_ping()),
                        "key_2".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_3".to_string() => A2AMessage::ConnectionResponse(_signed_response())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_2", uid);
                    assert_match!(A2AMessage::ConnectionRequest(_), message);
                }

                // Connection Problem Report
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::Ping(_ping()),
                        "key_2".to_string() => A2AMessage::Ack(_ack()),
                        "key_3".to_string() => A2AMessage::ConnectionProblemReport(_problem_report())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_3", uid);
                    assert_match!(A2AMessage::ConnectionProblemReport(_), message);
                }

                // No messages
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::Ping(_ping()),
                        "key_2".to_string() => A2AMessage::Ack(_ack())
                    );

                    assert!(connection.find_message_to_handle(messages).is_none());
                }
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_find_message_to_handle_from_responded_state() {
                let _setup = SetupIndyMocks::init();

                let connection = inviter_sm().to_inviter_responded_state();

                // Ping
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::Ping(_ping()),
                        "key_2".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_3".to_string() => A2AMessage::ConnectionResponse(_signed_response())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_1", uid);
                    assert_match!(A2AMessage::Ping(_), message);
                }

                // Ack
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::Ack(_ack()),
                        "key_3".to_string() => A2AMessage::ConnectionResponse(_signed_response())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_2", uid);
                    assert_match!(A2AMessage::Ack(_), message);
                }

                // Connection Problem Report
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionProblemReport(_problem_report())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_2", uid);
                    assert_match!(A2AMessage::ConnectionProblemReport(_), message);
                }

                // No messages
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response())
                    );

                    assert!(connection.find_message_to_handle(messages).is_none());
                }
            }

            #[test]
            #[cfg(feature = "general_test")]
            fn test_find_message_to_handle_from_completed_state() {
                let _setup = SetupIndyMocks::init();

                let connection = inviter_sm().to_inviter_completed_state();

                // Ping
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response()),
                        "key_3".to_string() => A2AMessage::ConnectionProblemReport(_problem_report()),
                        "key_4".to_string() => A2AMessage::Ping(_ping()),
                        "key_5".to_string() => A2AMessage::Ack(_ack())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_4", uid);
                    assert_match!(A2AMessage::Ping(_), message);
                }

                // Ping Response
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response()),
                        "key_3".to_string() => A2AMessage::ConnectionProblemReport(_problem_report()),
                        "key_4".to_string() => A2AMessage::PingResponse(_ping_response()),
                        "key_5".to_string() => A2AMessage::Ack(_ack())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_4", uid);
                    assert_match!(A2AMessage::PingResponse(_), message);
                }

                // Query
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response()),
                        "key_3".to_string() => A2AMessage::Query(_query())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_3", uid);
                    assert_match!(A2AMessage::Query(_), message);
                }

                // Disclose
                {
                    let messages = map!(
                        "key_1".to_string() => A2AMessage::ConnectionRequest(_request()),
                        "key_2".to_string() => A2AMessage::ConnectionResponse(_signed_response()),
                        "key_3".to_string() => A2AMessage::Disclose(_disclose())
                    );

                    let (uid, message) = connection.find_message_to_handle(messages).unwrap();
                    assert_eq!("key_3", uid);
                    assert_match!(A2AMessage::Disclose(_), message);
                }
            }
        }

        mod get_state {
            use super::*;

            #[test]
            #[cfg(feature = "general_test")]
            fn test_get_state() {
                let _setup = SetupMocks::init();

                assert_eq!(VcxStateType::VcxStateInitialized as u32, inviter_sm().state());
                assert_eq!(VcxStateType::VcxStateOfferSent as u32, inviter_sm().to_inviter_invited_state().state());
                assert_eq!(VcxStateType::VcxStateRequestReceived as u32, inviter_sm().to_inviter_responded_state().state());
                assert_eq!(VcxStateType::VcxStateAccepted as u32, inviter_sm().to_inviter_completed_state().state());
            }
        }
    }
}
