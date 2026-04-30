use anchor_lang::prelude::*;

use crate::errors::VelaError;

pub fn close_program_account<'info>(
    account_info: &AccountInfo<'info>,
    refund_destination: &AccountInfo<'info>,
) -> Result<()> {
    require_keys_eq!(*account_info.owner, crate::ID);

    let refund = account_info.lamports();
    if refund > 0 {
        **refund_destination.lamports.borrow_mut() = refund_destination
            .lamports()
            .checked_add(refund)
            .ok_or(VelaError::Overflow)?;
        **account_info.lamports.borrow_mut() = 0;
    }

    {
        let mut data = account_info.try_borrow_mut_data()?;
        data.fill(0);
    }
    account_info.resize(0)?;
    account_info.assign(&anchor_lang::system_program::ID);

    Ok(())
}
