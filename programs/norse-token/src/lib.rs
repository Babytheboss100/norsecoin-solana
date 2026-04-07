use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo, Transfer};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("NRSEtokenXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

// ============================================================================
//  Constants
// ============================================================================

/// Total supply: 1 trillion tokens with 9 decimals
pub const TOTAL_SUPPLY: u64 = 1_000_000_000_000 * 1_000_000_000; // 1e12 * 1e9
pub const TOKEN_DECIMALS: u8 = 9;
pub const DECIMALS_FACTOR: u64 = 1_000_000_000; // 10^9

/// Viking tier thresholds (adjusted for 9 decimals)
pub const TIER_THRESHOLDS: [u64; 6] = [
    0,                                      // Settler
    100_000 * DECIMALS_FACTOR,              // Thrall
    1_000_000 * DECIMALS_FACTOR,            // Karl
    10_000_000 * DECIMALS_FACTOR,           // Hersir
    100_000_000 * DECIMALS_FACTOR,          // Jarl
    1_000_000_000 * DECIMALS_FACTOR,        // Konungr
];

/// Nine Realms staking configuration: (lock_days, apy_bps)
pub const REALM_CONFIGS: [(u16, u16); 9] = [
    (7,   6000),   // Midgard    -  60% APY
    (14,  8000),   // Asgard     -  80% APY
    (30,  10000),  // Vanaheim   - 100% APY
    (60,  15000),  // Alfheim    - 150% APY
    (90,  20000),  // Svartalfheim - 200% APY
    (120, 25000),  // Nidavellir - 250% APY
    (180, 30000),  // Jotunheim  - 300% APY
    (270, 40000),  // Niflheim   - 400% APY
    (365, 50000),  // Muspelheim - 500% APY
];

pub const BPS: u64 = 10_000;
pub const SECONDS_PER_DAY: i64 = 86_400;
pub const SECONDS_PER_YEAR: i64 = 365 * SECONDS_PER_DAY;
pub const EARLY_WITHDRAWAL_PENALTY_BPS: u64 = 2_500; // 25%

/// Minimum stake: 100 NORSE tokens
pub const MIN_STAKE_AMOUNT: u64 = 100 * DECIMALS_FACTOR;

/// NFT constants
pub const NFT_MAX_SUPPLY: u64 = 10_000;
pub const NFT_MAX_PER_WALLET: u8 = 5;
pub const NFT_MINT_PRICE_LAMPORTS: u64 = 500_000_000; // 0.5 SOL

/// Presale stage prices in lamports per token (with 9 decimals)
/// Stage 1: 0.00001 SOL per token = 10_000 lamports per token
/// Stage 2: 0.00002 SOL per token = 20_000 lamports per token
/// Stage 3: 0.00003 SOL per token = 30_000 lamports per token
pub const PRESALE_PRICES: [u64; 3] = [10_000, 20_000, 30_000];

/// Presale cap per stage: 100 billion tokens (with 9 decimals)
pub const PRESALE_STAGE_CAP: u64 = 100_000_000_000 * DECIMALS_FACTOR;

// ============================================================================
//  Program
// ============================================================================

#[program]
pub mod norse_token {
    use super::*;

    // ────────────────────────────────────────────────
    //  Token Initialization
    // ────────────────────────────────────────────────

    /// Create the NORSE SPL token mint and mint the full supply to the deployer.
    pub fn initialize_token(ctx: Context<InitializeToken>) -> Result<()> {
        // Mint total supply to deployer's token account
        let seeds = &[b"mint-authority".as_ref(), &[ctx.bumps.mint_authority]];
        let signer_seeds = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.deployer_token_account.to_account_info(),
                    authority: ctx.accounts.mint_authority.to_account_info(),
                },
                signer_seeds,
            ),
            TOTAL_SUPPLY,
        )?;

        msg!("NorseCoin initialized: 1 trillion tokens minted");
        Ok(())
    }

    // ────────────────────────────────────────────────
    //  Viking Tier (view)
    // ────────────────────────────────────────────────

    /// Returns the Viking tier index (0-5) based on a token balance.
    /// 0=Settler, 1=Thrall, 2=Karl, 3=Hersir, 4=Jarl, 5=Konungr
    pub fn get_viking_tier(ctx: Context<GetVikingTier>) -> Result<u8> {
        let balance = ctx.accounts.user_token_account.amount;
        let tier = calculate_tier(balance);
        msg!("Viking tier for balance {}: {}", balance, tier_name(tier));
        Ok(tier)
    }

    // ────────────────────────────────────────────────
    //  Staking
    // ────────────────────────────────────────────────

    /// Initialize the staking pool with the Nine Realms configuration.
    pub fn initialize_staking(ctx: Context<InitializeStaking>) -> Result<()> {
        let pool = &mut ctx.accounts.staking_pool;
        pool.authority = ctx.accounts.authority.key();
        pool.token_mint = ctx.accounts.token_mint.key();
        pool.total_staked = 0;
        pool.reward_pool = 0;
        pool.bump = ctx.bumps.staking_pool;

        // Initialize the 9 realms
        for i in 0..9 {
            pool.realms[i] = RealmConfig {
                lock_days: REALM_CONFIGS[i].0,
                apy_bps: REALM_CONFIGS[i].1,
                enabled: true,
            };
        }

        msg!("Staking pool initialized with Nine Realms");
        Ok(())
    }

    /// Fund the reward pool so stakers can claim rewards.
    pub fn fund_reward_pool(ctx: Context<FundRewardPool>, amount: u64) -> Result<()> {
        require!(amount > 0, NorseError::ZeroAmount);

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.funder_token_account.to_account_info(),
                    to: ctx.accounts.staking_vault.to_account_info(),
                    authority: ctx.accounts.funder.to_account_info(),
                },
            ),
            amount,
        )?;

        let pool = &mut ctx.accounts.staking_pool;
        pool.reward_pool = pool.reward_pool.checked_add(amount).unwrap();

        msg!("Reward pool funded with {} tokens", amount);
        Ok(())
    }

    /// Stake NORSE tokens into a specific realm.
    pub fn stake(ctx: Context<StakeTokens>, realm_id: u8, amount: u64) -> Result<()> {
        require!(amount >= MIN_STAKE_AMOUNT, NorseError::BelowMinimumStake);
        require!((realm_id as usize) < 9, NorseError::InvalidRealm);

        let pool = &ctx.accounts.staking_pool;
        require!(pool.realms[realm_id as usize].enabled, NorseError::RealmDisabled);

        // Transfer tokens from user to staking vault
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.staking_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        // Initialize user stake account
        let user_stake = &mut ctx.accounts.user_stake;
        user_stake.owner = ctx.accounts.user.key();
        user_stake.realm_id = realm_id;
        user_stake.amount = amount;
        user_stake.start_time = Clock::get()?.unix_timestamp;
        user_stake.last_claim_time = Clock::get()?.unix_timestamp;
        user_stake.claimed_rewards = 0;
        user_stake.active = true;
        user_stake.bump = ctx.bumps.user_stake;

        // Update pool totals
        let pool = &mut ctx.accounts.staking_pool;
        pool.total_staked = pool.total_staked.checked_add(amount).unwrap();

        msg!("Staked {} tokens in realm {}", amount, realm_id);
        Ok(())
    }

    /// Unstake tokens after the lock period has elapsed. Claims pending rewards.
    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        let user_stake = &ctx.accounts.user_stake;
        require!(user_stake.active, NorseError::StakeNotActive);

        let pool = &ctx.accounts.staking_pool;
        let realm = &pool.realms[user_stake.realm_id as usize];
        let lock_end = user_stake.start_time + (realm.lock_days as i64 * SECONDS_PER_DAY);
        let now = Clock::get()?.unix_timestamp;
        require!(now >= lock_end, NorseError::LockPeriodNotEnded);

        let amount = user_stake.amount;

        // Calculate pending rewards
        let reward = calculate_pending_rewards(
            user_stake.amount,
            realm.apy_bps,
            user_stake.last_claim_time,
            now,
        );

        // Transfer staked tokens + rewards back to user
        let pool_bump = ctx.accounts.staking_pool.bump;
        let seeds = &[b"staking-pool".as_ref(), &[pool_bump]];
        let signer_seeds = &[&seeds[..]];

        let total_return = amount.checked_add(reward).unwrap_or(amount);
        let actual_reward = if ctx.accounts.staking_pool.reward_pool >= reward {
            reward
        } else {
            0
        };
        let transfer_amount = amount.checked_add(actual_reward).unwrap();

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.staking_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.staking_pool.to_account_info(),
                },
                signer_seeds,
            ),
            transfer_amount,
        )?;

        // Update state
        let pool = &mut ctx.accounts.staking_pool;
        pool.total_staked = pool.total_staked.checked_sub(amount).unwrap();
        if actual_reward > 0 {
            pool.reward_pool = pool.reward_pool.checked_sub(actual_reward).unwrap();
        }

        let user_stake = &mut ctx.accounts.user_stake;
        user_stake.active = false;
        user_stake.claimed_rewards = user_stake.claimed_rewards.checked_add(actual_reward).unwrap();

        msg!("Unstaked {} tokens, claimed {} rewards", amount, actual_reward);
        Ok(())
    }

    /// Claim accumulated staking rewards without unstaking.
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let user_stake = &ctx.accounts.user_stake;
        require!(user_stake.active, NorseError::StakeNotActive);

        let pool = &ctx.accounts.staking_pool;
        let realm = &pool.realms[user_stake.realm_id as usize];
        let now = Clock::get()?.unix_timestamp;

        let reward = calculate_pending_rewards(
            user_stake.amount,
            realm.apy_bps,
            user_stake.last_claim_time,
            now,
        );
        require!(reward > 0, NorseError::NoRewards);
        require!(pool.reward_pool >= reward, NorseError::InsufficientRewardPool);

        // Transfer rewards
        let pool_bump = ctx.accounts.staking_pool.bump;
        let seeds = &[b"staking-pool".as_ref(), &[pool_bump]];
        let signer_seeds = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.staking_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.staking_pool.to_account_info(),
                },
                signer_seeds,
            ),
            reward,
        )?;

        // Update state
        let pool = &mut ctx.accounts.staking_pool;
        pool.reward_pool = pool.reward_pool.checked_sub(reward).unwrap();

        let user_stake = &mut ctx.accounts.user_stake;
        user_stake.last_claim_time = now;
        user_stake.claimed_rewards = user_stake.claimed_rewards.checked_add(reward).unwrap();

        msg!("Claimed {} reward tokens", reward);
        Ok(())
    }

    /// Emergency unstake before lock period ends. 25% penalty on principal.
    pub fn emergency_unstake(ctx: Context<Unstake>) -> Result<()> {
        let user_stake = &ctx.accounts.user_stake;
        require!(user_stake.active, NorseError::StakeNotActive);

        let amount = user_stake.amount;
        let penalty = amount
            .checked_mul(EARLY_WITHDRAWAL_PENALTY_BPS)
            .unwrap()
            .checked_div(BPS)
            .unwrap();
        let return_amount = amount.checked_sub(penalty).unwrap();

        // Transfer reduced amount back to user
        let pool_bump = ctx.accounts.staking_pool.bump;
        let seeds = &[b"staking-pool".as_ref(), &[pool_bump]];
        let signer_seeds = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.staking_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.staking_pool.to_account_info(),
                },
                signer_seeds,
            ),
            return_amount,
        )?;

        // Penalty goes to reward pool
        let pool = &mut ctx.accounts.staking_pool;
        pool.total_staked = pool.total_staked.checked_sub(amount).unwrap();
        pool.reward_pool = pool.reward_pool.checked_add(penalty).unwrap();

        let user_stake = &mut ctx.accounts.user_stake;
        user_stake.active = false;

        msg!(
            "Emergency unstake: returned {}, penalty {} added to reward pool",
            return_amount,
            penalty
        );
        Ok(())
    }

    // ────────────────────────────────────────────────
    //  NFT Collection
    // ────────────────────────────────────────────────

    /// Initialize the NFT collection state.
    pub fn initialize_nft_collection(ctx: Context<InitializeNftCollection>) -> Result<()> {
        let collection = &mut ctx.accounts.nft_collection;
        collection.authority = ctx.accounts.authority.key();
        collection.total_minted = 0;
        collection.mint_price = NFT_MINT_PRICE_LAMPORTS;
        collection.minting_enabled = false;
        collection.bump = ctx.bumps.nft_collection;

        msg!("Norse Viking Artifacts NFT collection initialized");
        Ok(())
    }

    /// Mint a Viking Artifact NFT. Costs 0.5 SOL.
    pub fn mint_nft(ctx: Context<MintNft>) -> Result<()> {
        let collection = &ctx.accounts.nft_collection;
        require!(collection.minting_enabled, NorseError::MintingNotEnabled);
        require!(collection.total_minted < NFT_MAX_SUPPLY, NorseError::MaxSupplyReached);

        let user_nft_account = &ctx.accounts.user_nft_account;
        require!(
            user_nft_account.mint_count < NFT_MAX_PER_WALLET,
            NorseError::MaxPerWalletReached
        );

        // Transfer SOL payment from user to treasury
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.treasury.key(),
            collection.mint_price,
        );
        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
            ],
        )?;

        // Mint the NFT token (1 token to the user's ATA for this mint)
        let collection_bump = ctx.accounts.nft_collection.bump;
        let seeds = &[b"nft-collection".as_ref(), &[collection_bump]];
        let signer_seeds = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.nft_mint.to_account_info(),
                    to: ctx.accounts.user_nft_token_account.to_account_info(),
                    authority: ctx.accounts.nft_collection.to_account_info(),
                },
                signer_seeds,
            ),
            1,
        )?;

        // Update state
        let collection = &mut ctx.accounts.nft_collection;
        collection.total_minted += 1;

        let user_nft_account = &mut ctx.accounts.user_nft_account;
        user_nft_account.mint_count += 1;

        msg!(
            "Minted Viking Artifact NFT #{} to {}",
            collection.total_minted,
            ctx.accounts.user.key()
        );
        Ok(())
    }

    /// Toggle minting on/off (authority only).
    pub fn toggle_nft_minting(ctx: Context<ToggleNftMinting>) -> Result<()> {
        let collection = &mut ctx.accounts.nft_collection;
        collection.minting_enabled = !collection.minting_enabled;
        msg!("NFT minting enabled: {}", collection.minting_enabled);
        Ok(())
    }

    // ────────────────────────────────────────────────
    //  Presale
    // ────────────────────────────────────────────────

    /// Initialize the presale with 3 stages.
    pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
        let presale = &mut ctx.accounts.presale_state;
        presale.authority = ctx.accounts.authority.key();
        presale.token_mint = ctx.accounts.token_mint.key();
        presale.current_stage = 0;
        presale.tge_time = 0;
        presale.bump = ctx.bumps.presale_state;

        // Stage 0 - Ragnarok: 0.00001 SOL/token, 100B cap, not active
        presale.stages[0] = PresaleStage {
            price_lamports_per_token: PRESALE_PRICES[0],
            total_cap: PRESALE_STAGE_CAP,
            sold: 0,
            active: false,
        };

        // Stage 1 - Valhalla: 0.00002 SOL/token, 100B cap, not active
        presale.stages[1] = PresaleStage {
            price_lamports_per_token: PRESALE_PRICES[1],
            total_cap: PRESALE_STAGE_CAP,
            sold: 0,
            active: false,
        };

        // Stage 2 - Odin's Blessing: 0.00003 SOL/token, 100B cap, not active
        presale.stages[2] = PresaleStage {
            price_lamports_per_token: PRESALE_PRICES[2],
            total_cap: PRESALE_STAGE_CAP,
            sold: 0,
            active: false,
        };

        msg!("Presale initialized with 3 stages");
        Ok(())
    }

    /// Activate a presale stage (authority only).
    pub fn set_stage_active(ctx: Context<PresaleAdmin>, stage: u8, active: bool) -> Result<()> {
        require!((stage as usize) < 3, NorseError::InvalidPresaleStage);
        let presale = &mut ctx.accounts.presale_state;
        presale.stages[stage as usize].active = active;
        presale.current_stage = stage;
        msg!("Presale stage {} active: {}", stage, active);
        Ok(())
    }

    /// Set TGE time for vesting schedule (authority only).
    pub fn set_tge_time(ctx: Context<PresaleAdmin>, tge_time: i64) -> Result<()> {
        require!(tge_time > 0, NorseError::InvalidTgeTime);
        let presale = &mut ctx.accounts.presale_state;
        presale.tge_time = tge_time;
        msg!("TGE time set to {}", tge_time);
        Ok(())
    }

    /// Buy tokens in the current presale stage by sending SOL.
    pub fn buy_presale(ctx: Context<BuyPresale>, sol_amount: u64) -> Result<()> {
        require!(sol_amount > 0, NorseError::ZeroAmount);

        let presale = &ctx.accounts.presale_state;
        let stage_idx = presale.current_stage as usize;
        require!(stage_idx < 3, NorseError::InvalidPresaleStage);

        let stage = &presale.stages[stage_idx];
        require!(stage.active, NorseError::StageNotActive);

        // Calculate token amount: sol_amount (lamports) / price_per_token (lamports)
        // Then multiply by DECIMALS_FACTOR to get the amount in smallest units
        // price is in lamports per 1 whole token, so:
        // token_amount = (sol_amount / price_lamports_per_token) * DECIMALS_FACTOR
        let token_amount = (sol_amount as u128)
            .checked_mul(DECIMALS_FACTOR as u128)
            .unwrap()
            .checked_div(stage.price_lamports_per_token as u128)
            .unwrap() as u64;

        require!(token_amount > 0, NorseError::AmountTooSmall);
        require!(
            stage.sold.checked_add(token_amount).unwrap() <= stage.total_cap,
            NorseError::ExceedsStageCap
        );

        // Transfer SOL from buyer to presale vault
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.buyer.key(),
            &ctx.accounts.presale_vault.key(),
            sol_amount,
        );
        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.presale_vault.to_account_info(),
            ],
        )?;

        // Update presale state
        let presale = &mut ctx.accounts.presale_state;
        presale.stages[stage_idx].sold = presale.stages[stage_idx]
            .sold
            .checked_add(token_amount)
            .unwrap();

        // Update user presale account
        let user_presale = &mut ctx.accounts.user_presale;
        if user_presale.owner == Pubkey::default() {
            user_presale.owner = ctx.accounts.buyer.key();
            user_presale.purchase_time = Clock::get()?.unix_timestamp;
        }
        user_presale.total_purchased = user_presale
            .total_purchased
            .checked_add(token_amount)
            .unwrap();
        user_presale.bump = ctx.bumps.user_presale;

        msg!(
            "Purchased {} tokens for {} lamports in stage {}",
            token_amount,
            sol_amount,
            stage_idx
        );
        Ok(())
    }

    /// Claim vested tokens: 25% at TGE, 25% at TGE+30d, 25% at TGE+60d, 25% at TGE+90d.
    pub fn claim_vested(ctx: Context<ClaimVested>) -> Result<()> {
        let presale = &ctx.accounts.presale_state;
        require!(presale.tge_time > 0, NorseError::TgeNotStarted);

        let now = Clock::get()?.unix_timestamp;
        require!(now >= presale.tge_time, NorseError::TgeNotStarted);

        let user_presale = &ctx.accounts.user_presale;
        require!(user_presale.total_purchased > 0, NorseError::NothingToClaim);

        let vested = calculate_vested_amount(
            user_presale.total_purchased,
            presale.tge_time,
            now,
        );

        let claimable = vested
            .checked_sub(user_presale.total_claimed)
            .unwrap_or(0);
        require!(claimable > 0, NorseError::NothingToClaim);

        // Transfer vested tokens from presale token vault to user
        let presale_bump = ctx.accounts.presale_state.bump;
        let seeds = &[b"presale-state".as_ref(), &[presale_bump]];
        let signer_seeds = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.presale_token_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.presale_state.to_account_info(),
                },
                signer_seeds,
            ),
            claimable,
        )?;

        // Update user presale state
        let user_presale = &mut ctx.accounts.user_presale;
        user_presale.total_claimed = user_presale
            .total_claimed
            .checked_add(claimable)
            .unwrap();

        msg!("Claimed {} vested tokens", claimable);
        Ok(())
    }
}

// ============================================================================
//  Helper Functions
// ============================================================================

fn calculate_tier(balance: u64) -> u8 {
    for i in (1..=5).rev() {
        if balance >= TIER_THRESHOLDS[i] {
            return i as u8;
        }
    }
    0 // Settler
}

fn tier_name(tier: u8) -> &'static str {
    match tier {
        0 => "Settler",
        1 => "Thrall",
        2 => "Karl",
        3 => "Hersir",
        4 => "Jarl",
        5 => "Konungr",
        _ => "Unknown",
    }
}

fn calculate_pending_rewards(amount: u64, apy_bps: u16, last_claim_time: i64, now: i64) -> u64 {
    let elapsed = now.saturating_sub(last_claim_time) as u64;
    // reward = principal * apy_bps / 10000 * elapsed / SECONDS_PER_YEAR
    (amount as u128)
        .checked_mul(apy_bps as u128)
        .unwrap()
        .checked_mul(elapsed as u128)
        .unwrap()
        .checked_div(BPS as u128 * SECONDS_PER_YEAR as u128)
        .unwrap_or(0) as u64
}

fn calculate_vested_amount(total: u64, tge_time: i64, now: i64) -> u64 {
    if now < tge_time {
        return 0;
    }
    let elapsed = now - tge_time;
    let tranche = total / 4; // 25% per tranche

    if elapsed >= 90 * SECONDS_PER_DAY {
        total          // 100%
    } else if elapsed >= 60 * SECONDS_PER_DAY {
        tranche * 3    // 75%
    } else if elapsed >= 30 * SECONDS_PER_DAY {
        tranche * 2    // 50%
    } else {
        tranche        // 25% at TGE
    }
}

// ============================================================================
//  Account Structures
// ============================================================================

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct RealmConfig {
    pub lock_days: u16,
    pub apy_bps: u16,
    pub enabled: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct PresaleStage {
    pub price_lamports_per_token: u64,
    pub total_cap: u64,
    pub sold: u64,
    pub active: bool,
}

#[account]
pub struct StakingPool {
    pub authority: Pubkey,     // 32
    pub token_mint: Pubkey,    // 32
    pub total_staked: u64,     // 8
    pub reward_pool: u64,      // 8
    pub realms: [RealmConfig; 9], // 9 * 5 = 45
    pub bump: u8,              // 1
}
// Space: 8 (discriminator) + 32 + 32 + 8 + 8 + 45 + 1 = 134

#[account]
pub struct UserStake {
    pub owner: Pubkey,          // 32
    pub realm_id: u8,           // 1
    pub amount: u64,            // 8
    pub start_time: i64,        // 8
    pub last_claim_time: i64,   // 8
    pub claimed_rewards: u64,   // 8
    pub active: bool,           // 1
    pub bump: u8,               // 1
}
// Space: 8 + 32 + 1 + 8 + 8 + 8 + 8 + 1 + 1 = 75

#[account]
pub struct NftCollection {
    pub authority: Pubkey,      // 32
    pub total_minted: u64,      // 8
    pub mint_price: u64,        // 8
    pub minting_enabled: bool,  // 1
    pub bump: u8,               // 1
}
// Space: 8 + 32 + 8 + 8 + 1 + 1 = 58

#[account]
pub struct UserNftAccount {
    pub owner: Pubkey,          // 32
    pub mint_count: u8,         // 1
    pub bump: u8,               // 1
}
// Space: 8 + 32 + 1 + 1 = 42

#[account]
pub struct PresaleState {
    pub authority: Pubkey,      // 32
    pub token_mint: Pubkey,     // 32
    pub current_stage: u8,      // 1
    pub tge_time: i64,          // 8
    pub stages: [PresaleStage; 3], // 3 * 25 = 75
    pub bump: u8,               // 1
}
// Space: 8 + 32 + 32 + 1 + 8 + 75 + 1 = 157

#[account]
pub struct UserPresale {
    pub owner: Pubkey,           // 32
    pub total_purchased: u64,    // 8
    pub total_claimed: u64,      // 8
    pub purchase_time: i64,      // 8
    pub bump: u8,                // 1
}
// Space: 8 + 32 + 8 + 8 + 8 + 1 = 65

// ============================================================================
//  Instruction Contexts
// ============================================================================

#[derive(Accounts)]
pub struct InitializeToken<'info> {
    #[account(mut)]
    pub deployer: Signer<'info>,

    #[account(
        init,
        payer = deployer,
        mint::decimals = TOKEN_DECIMALS,
        mint::authority = mint_authority,
    )]
    pub token_mint: Account<'info, Mint>,

    /// CHECK: PDA used as mint authority
    #[account(
        seeds = [b"mint-authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = deployer,
        associated_token::mint = token_mint,
        associated_token::authority = deployer,
    )]
    pub deployer_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct GetVikingTier<'info> {
    pub user_token_account: Account<'info, TokenAccount>,
}

#[derive(Accounts)]
pub struct InitializeStaking<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 32 + 8 + 8 + (9 * 5) + 1,
        seeds = [b"staking-pool"],
        bump,
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        init,
        payer = authority,
        token::mint = token_mint,
        token::authority = staking_pool,
        seeds = [b"staking-vault"],
        bump,
    )]
    pub staking_vault: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct FundRewardPool<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(
        mut,
        seeds = [b"staking-pool"],
        bump = staking_pool.bump,
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        seeds = [b"staking-vault"],
        bump,
    )]
    pub staking_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(realm_id: u8, amount: u64)]
pub struct StakeTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"staking-pool"],
        bump = staking_pool.bump,
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        seeds = [b"staking-vault"],
        bump,
    )]
    pub staking_vault: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = user,
        space = 8 + 32 + 1 + 8 + 8 + 8 + 8 + 1 + 1,
        seeds = [b"user-stake", user.key().as_ref(), &[realm_id]],
        bump,
    )]
    pub user_stake: Account<'info, UserStake>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"staking-pool"],
        bump = staking_pool.bump,
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        seeds = [b"staking-vault"],
        bump,
    )]
    pub staking_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"user-stake", user.key().as_ref(), &[user_stake.realm_id]],
        bump = user_stake.bump,
        has_one = owner @ NorseError::Unauthorized,
    )]
    pub user_stake: Account<'info, UserStake>,

    /// CHECK: Validated by has_one constraint
    pub owner: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"staking-pool"],
        bump = staking_pool.bump,
    )]
    pub staking_pool: Account<'info, StakingPool>,

    #[account(
        mut,
        seeds = [b"staking-vault"],
        bump,
    )]
    pub staking_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"user-stake", user.key().as_ref(), &[user_stake.realm_id]],
        bump = user_stake.bump,
        has_one = owner @ NorseError::Unauthorized,
    )]
    pub user_stake: Account<'info, UserStake>,

    /// CHECK: Validated by has_one constraint
    pub owner: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct InitializeNftCollection<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 1 + 1,
        seeds = [b"nft-collection"],
        bump,
    )]
    pub nft_collection: Account<'info, NftCollection>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintNft<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"nft-collection"],
        bump = nft_collection.bump,
    )]
    pub nft_collection: Account<'info, NftCollection>,

    #[account(
        init_if_needed,
        payer = user,
        space = 8 + 32 + 1 + 1,
        seeds = [b"user-nft", user.key().as_ref()],
        bump,
    )]
    pub user_nft_account: Account<'info, UserNftAccount>,

    /// The NFT mint account (created by the client before calling this instruction)
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,

    /// The user's token account for this specific NFT mint
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = nft_mint,
        associated_token::authority = user,
    )]
    pub user_nft_token_account: Account<'info, TokenAccount>,

    /// CHECK: Treasury wallet to receive SOL payment
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ToggleNftMinting<'info> {
    #[account(
        mut,
        seeds = [b"nft-collection"],
        bump = nft_collection.bump,
        has_one = authority @ NorseError::Unauthorized,
    )]
    pub nft_collection: Account<'info, NftCollection>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct InitializePresale<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 32 + 1 + 8 + (3 * 25) + 1,
        seeds = [b"presale-state"],
        bump,
    )]
    pub presale_state: Account<'info, PresaleState>,

    /// Token vault to hold presale tokens
    #[account(
        init,
        payer = authority,
        token::mint = token_mint,
        token::authority = presale_state,
        seeds = [b"presale-token-vault"],
        bump,
    )]
    pub presale_token_vault: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct PresaleAdmin<'info> {
    #[account(
        mut,
        seeds = [b"presale-state"],
        bump = presale_state.bump,
        has_one = authority @ NorseError::Unauthorized,
    )]
    pub presale_state: Account<'info, PresaleState>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct BuyPresale<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"presale-state"],
        bump = presale_state.bump,
    )]
    pub presale_state: Account<'info, PresaleState>,

    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + 32 + 8 + 8 + 8 + 1,
        seeds = [b"user-presale", buyer.key().as_ref()],
        bump,
    )]
    pub user_presale: Account<'info, UserPresale>,

    /// CHECK: SOL vault for presale proceeds
    #[account(
        mut,
        seeds = [b"presale-vault"],
        bump,
    )]
    pub presale_vault: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimVested<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"presale-state"],
        bump = presale_state.bump,
    )]
    pub presale_state: Account<'info, PresaleState>,

    #[account(
        mut,
        seeds = [b"user-presale", user.key().as_ref()],
        bump = user_presale.bump,
        has_one = owner @ NorseError::Unauthorized,
    )]
    pub user_presale: Account<'info, UserPresale>,

    /// CHECK: Validated by has_one constraint
    pub owner: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"presale-token-vault"],
        bump,
    )]
    pub presale_token_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ============================================================================
//  Errors
// ============================================================================

#[error_code]
pub enum NorseError {
    #[msg("Amount is zero")]
    ZeroAmount,

    #[msg("Below minimum stake amount (100 NORSE)")]
    BelowMinimumStake,

    #[msg("Invalid realm ID (must be 0-8)")]
    InvalidRealm,

    #[msg("Realm is disabled")]
    RealmDisabled,

    #[msg("Stake is not active")]
    StakeNotActive,

    #[msg("Lock period has not ended")]
    LockPeriodNotEnded,

    #[msg("No rewards to claim")]
    NoRewards,

    #[msg("Insufficient reward pool balance")]
    InsufficientRewardPool,

    #[msg("NFT minting is not enabled")]
    MintingNotEnabled,

    #[msg("Max NFT supply reached (10,000)")]
    MaxSupplyReached,

    #[msg("Max NFTs per wallet reached (5)")]
    MaxPerWalletReached,

    #[msg("Invalid presale stage")]
    InvalidPresaleStage,

    #[msg("Presale stage is not active")]
    StageNotActive,

    #[msg("Amount too small")]
    AmountTooSmall,

    #[msg("Purchase exceeds stage cap")]
    ExceedsStageCap,

    #[msg("TGE has not started")]
    TgeNotStarted,

    #[msg("Nothing to claim")]
    NothingToClaim,

    #[msg("Invalid TGE time")]
    InvalidTgeTime,

    #[msg("Unauthorized")]
    Unauthorized,
}
