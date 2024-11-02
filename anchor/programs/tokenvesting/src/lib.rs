use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked}};

declare_id!("AtyRAoEYGD6bk7rs3xQAReHVCjXYSCbXBjSYCfKcm7WA");

#[program]
pub mod tokenvesting {
    use super::*;

  pub fn create_vesting_account(ctx: Context<CreateVestingAccount>, company_name: String) -> Result<()> {
    *ctx.accounts.vesting_account = VestingAccount {
      owner: ctx.accounts.signer.key(),
      mint: ctx.accounts.mint.key(),
      treasury_token_account: ctx.accounts.treasury_token_account.key(),
      company_name,
      treasury_bump: ctx.bumps.treasury_token_account,
      bump: ctx.bumps.vesting_account,
    };

    Ok(())
  }

  pub fn create_employee_account(
    ctx: Context<CreateEmployeeAccount>,
    start_time: i64,
    end_time: i64,
    total_amount: i64,
    cliff_time: i64,
  ) -> Result<()> {
    *ctx.accounts.employee_account = EmployeeAccount {
      beneficiary: ctx.accounts.beneficiary.key(),
      start_time,
      end_time,
      total_amount,
      total_withdrawn: 0,
      cliff_time,
      vesting_account: ctx.accounts.vesting_account.key(),
      bump: ctx.bumps.employee_account,
    };

    Ok(())
  }

  pub fn claim_tokens(ctx: Context<ClaimTokens>, _company_name: String, ) -> Result<()> {
    //get all info for vested token from employee account
    let employee_account = &mut ctx.accounts.employee_account;
  
    //time needed to check if employee can claim yet
    let now = Clock::get()?.unix_timestamp;
  
    if now < employee_account.cliff_time {
      return Err(ErrorCode::TooEarlyToClaim.into())
    }
  
    //prevent underflow and overflow in calculations
    let time_since_start = now.saturating_sub(employee_account.start_time);
    let total_vesting_time = employee_account.end_time.saturating_sub(employee_account.start_time);
  
    if total_vesting_time == 0 {
      return Err(ErrorCode::InvalidVestingPeriod.into())
    }
  
    let vested_amount = if now >= employee_account.end_time {
      employee_account.total_amount
    } else {
      match employee_account.total_amount.checked_mul(time_since_start as i64) {
        Some(product) => {
          product / total_vesting_time as i64
        },
        None => {
          return Err(ErrorCode::CalculationOverflow.into())
        }
      }
    };
  
    //amount user can withdraw
    let claimable_amount = vested_amount.saturating_sub(employee_account.total_withdrawn);
  
    if claimable_amount == 0 {
      return Err(ErrorCode::NothingToClaim.into())
    }
  
    //claim - define all accounts needed for transfer to take place
    let transfer_cpi_accounts = TransferChecked {
      from: ctx.accounts.treasury_token_account.to_account_info(),
      mint: ctx.accounts.mint.to_account_info(),
      to: ctx.accounts.employee_token_account.to_account_info(),
      authority: ctx.accounts.treasury_token_account.to_account_info(),
    };
  
    let cpi_program = ctx.accounts.token_program.to_account_info();
  
    let signer_seeds: &[&[&[u8]]] = &[
      &[b"vesting_treasury",
      ctx.accounts.vesting_account.company_name.as_ref(),
      &[ctx.accounts.vesting_account.treasury_bump]],
    ];
  
    let cpi_context = CpiContext::new(cpi_program, transfer_cpi_accounts).with_signer(signer_seeds);
  
    let decimals = ctx.accounts.mint.decimals;
  
    //cpi transferring tokens from treasury account to employee account
    token_interface::transfer_checked(cpi_context, claimable_amount as u64, decimals)?;
  
    // update employee account state
    employee_account.total_withdrawn += claimable_amount;
  
    Ok(())
  }

}


#[derive(Accounts)]
#[instruction(company_name: String)]
//context - passing through all accounts needed to process instruction(create_vesting_accounts) 
pub struct CreateVestingAccount<'info> {
  #[account(mut)]
  pub signer: Signer<'info>,

  #[account(
    init,
    space = 8 + VestingAccount::INIT_SPACE,
    payer = signer,
    //differentiate different pda's for specific signer
    seeds = [company_name.as_ref()],
    bump,
  )]
  //define what the account is
  pub vesting_account: Account<'info, VestingAccount>,

  pub mint: InterfaceAccount<'info, Mint>,

  //treasury token account storing for all tokens
  #[account(
    init,
    token::mint = mint,
    token::authority = treasury_token_account,
    payer = signer,
    seeds = [b"vesting_treasury", company_name.as_bytes()],
    bump,
  )]
  pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,

  
  //token program for when creating a token account
  pub token_program: Interface<'info, TokenInterface>,
  //must always pass system program when initializing new account on chain
  pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateEmployeeAccount<'info> {
  #[account(mut)]
  pub owner: Signer<'info>,
  pub beneficiary: SystemAccount<'info>,
  //ensuring only employer has access
  #[account(
    has_one = owner,
  )]
  pub vesting_account: Account<'info, VestingAccount>,
  #[account(
    init,
    space = 8 + EmployeeAccount::INIT_SPACE,
    payer = owner,
    seeds = [b"employee_vesting", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
    bump,
  )]
  pub employee_account: Account<'info, EmployeeAccount>,

  pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(company_name: String)]
pub struct ClaimTokens<'info> {
  #[account(mut)]
  pub beneficiary: Signer<'info>,

  #[account(
    mut,
    seeds = [b"employee_vesting", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
    bump = employee_account.bump,
    //ensure employee + correct vesting account
    has_one = beneficiary,
    has_one = vesting_account,
  )]
  pub employee_account: Account<'info, EmployeeAccount>,

  //grabbing the needed vesting account
  #[account(
    mut,
    seeds = [company_name.as_ref()],
    bump = vesting_account.bump,
    //ensure correct token account(treasury token account) + correct mint address of spl tokens
    has_one = treasury_token_account,
    has_one =  mint,
  )]
  pub vesting_account: Account<'info, VestingAccount>,

  pub mint: InterfaceAccount<'info, Mint>,

  #[account(mut)]
  pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,

  #[account(
    //check if user does not already have token account created for spl token
    init_if_needed,
    payer = beneficiary,
    associated_token::mint = mint,
    associated_token::authority = beneficiary,
    associated_token::token_program = token_program,
  )]
  pub employee_token_account: InterfaceAccount<'info, TokenAccount>,
  
  pub token_program: Interface<'info, TokenInterface>,

  pub associated_token_program: Program<'info, AssociatedToken>,

  pub system_program: Program<'info, System>
}

//account storing state
#[account]
#[derive(InitSpace)]
pub struct VestingAccount {
  pub owner: Pubkey,
  //mint for spl tokens distributed
  pub mint: Pubkey,
  //token account storing spl tokens
  pub treasury_token_account: Pubkey,
  #[max_len(50)]
  pub company_name: String,
  pub treasury_bump: u8,
  pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct EmployeeAccount {
  pub beneficiary: Pubkey,
  pub start_time: i64,
  pub end_time: i64,
  pub cliff_time: i64,
  pub vesting_account: Pubkey,
  pub total_amount: i64,
  pub total_withdrawn: i64,
  pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
  #[msg("Too early to claim tokens")]
  TooEarlyToClaim,

  #[msg("Invalid vesting period")]
  InvalidVestingPeriod,

  #[msg("Calculation oveflow")]
  CalculationOverflow,

  #[msg("Nothing to claim")]
  NothingToClaim
}