use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    state::{ArciumRequestFlow, ArciumRequestState, ArciumRequestStatus, CURRENT_ACCOUNT_VERSION},
};

pub struct StartArciumRequestArgs {
    pub mandate: Pubkey,
    pub flow: ArciumRequestFlow,
    pub subject: [u8; 8],
    pub computation_offset: u64,
    pub request_nonce: u64,
    pub bump: u8,
    pub now: i64,
}

pub fn start_arcium_request(
    request: &mut ArciumRequestState,
    args: StartArciumRequestArgs,
) -> Result<()> {
    if request.created_at != 0 && request.status == ArciumRequestStatus::Pending {
        return Err(VelaError::ArciumRequestAlreadyPending.into());
    }

    request.mandate = args.mandate;
    request.flow = args.flow;
    request.subject = args.subject;
    request.computation_offset = args.computation_offset;
    request.request_nonce = args.request_nonce;
    request.status = ArciumRequestStatus::Pending;
    request.created_at = args.now;
    request.completed_at = 0;
    request.bump = args.bump;
    request.version = CURRENT_ACCOUNT_VERSION;
    request._reserved = [0; 32];

    Ok(())
}

pub fn validate_pending_arcium_request(
    request: &ArciumRequestState,
    mandate: Pubkey,
    flow: ArciumRequestFlow,
    subject: [u8; 8],
) -> Result<u64> {
    require_keys_eq!(
        request.mandate,
        mandate,
        VelaError::InvalidArciumRequestState
    );
    require!(
        request.flow == flow
            && request.subject == subject
            && request.status == ArciumRequestStatus::Pending,
        VelaError::InvalidArciumRequestState
    );

    Ok(request.computation_offset)
}

pub fn complete_arcium_request(request: &mut ArciumRequestState, now: i64) -> Result<()> {
    require!(
        request.status == ArciumRequestStatus::Pending,
        VelaError::InvalidArciumRequestState
    );
    request.status = ArciumRequestStatus::Completed;
    request.completed_at = now;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_request() -> ArciumRequestState {
        ArciumRequestState {
            mandate: Pubkey::new_unique(),
            flow: ArciumRequestFlow::Validation,
            subject: 42i64.to_le_bytes(),
            computation_offset: 7,
            request_nonce: 3,
            status: ArciumRequestStatus::Pending,
            created_at: 100,
            completed_at: 0,
            bump: 255,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; 32],
        }
    }

    #[test]
    fn pending_request_rejects_duplicate_start() {
        let mut request = pending_request();
        let err = start_arcium_request(
            &mut request,
            StartArciumRequestArgs {
                mandate: Pubkey::new_unique(),
                flow: ArciumRequestFlow::Validation,
                subject: 43i64.to_le_bytes(),
                computation_offset: 8,
                request_nonce: 4,
                bump: 254,
                now: 101,
            },
        )
        .expect_err("pending request should reject duplicate start");

        assert!(format!("{err:?}").contains("ArciumRequestAlreadyPending"));
    }

    #[test]
    fn completed_request_can_be_reused_for_new_nonce() {
        let mut request = pending_request();
        complete_arcium_request(&mut request, 101).expect("completion should succeed");

        let mandate = Pubkey::new_unique();
        start_arcium_request(
            &mut request,
            StartArciumRequestArgs {
                mandate,
                flow: ArciumRequestFlow::UsageComputation,
                subject: 12u64.to_le_bytes(),
                computation_offset: 9,
                request_nonce: 5,
                bump: 253,
                now: 102,
            },
        )
        .expect("completed request state should be reusable");

        assert_eq!(request.mandate, mandate);
        assert_eq!(request.flow, ArciumRequestFlow::UsageComputation);
        assert_eq!(request.computation_offset, 9);
        assert_eq!(request.status, ArciumRequestStatus::Pending);
        assert_eq!(request.completed_at, 0);
    }

    #[test]
    fn pending_validation_returns_stored_offset_and_rejects_wrong_binding() {
        let request = pending_request();

        let offset = validate_pending_arcium_request(
            &request,
            request.mandate,
            ArciumRequestFlow::Validation,
            request.subject,
        )
        .expect("matching request should validate");
        assert_eq!(offset, request.computation_offset);

        let err = validate_pending_arcium_request(
            &request,
            request.mandate,
            ArciumRequestFlow::BillingRecord,
            request.subject,
        )
        .expect_err("wrong flow should be rejected");
        assert!(format!("{err:?}").contains("InvalidArciumRequestState"));
    }
}
