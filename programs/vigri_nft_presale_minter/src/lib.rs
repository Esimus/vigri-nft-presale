#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
    metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata},
};

// DataV2 re-exported via anchor_spl::metadata
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;


declare_id!("GmrUAwBvC3ijaM2L7kjddQFMWHevxRnArngf7jFx1yEk");

#[program]
pub mod vigri_nft_presale_minter {
    use super::*;

    // One-time global initialization
    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
        let global_config = &mut ctx.accounts.global_config;

        global_config.admin = args.admin;
        global_config.collection_mint = args.collection_mint;
        global_config.payment_mint = args.payment_mint;

        global_config.is_sales_paused = false;
        global_config.tiers = GlobalConfig::default_tiers();

        Ok(())
    }

    // Admin config update (prices, flags, pause)
    pub fn update_config(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
        let global_config = &mut ctx.accounts.global_config;

        // Extra safety, even though we also have `has_one = admin` in accounts
        require_keys_eq!(
            global_config.admin,
            ctx.accounts.admin.key(),
            PresaleError::Unauthorized
        );

        // 1) Global pause flag
        if let Some(paused) = args.is_sales_paused {
            global_config.is_sales_paused = paused;
        }

        // 2) Per-tier updates (optional)
        if let Some(tier_id) = args.tier_id {
            let idx = tier_id as usize;
            require!(idx < global_config.tiers.len(), PresaleError::InvalidTierId);

            let tier = &mut global_config.tiers[idx];

            if let Some(price) = args.new_price_lamports {
                tier.price_lamports = price;
            }
            if let Some(kyc) = args.new_kyc_required {
                tier.kyc_required = kyc;
            }
            if let Some(invite_only) = args.new_invite_only {
                tier.invite_only = invite_only;
            }
            if let Some(transferable) = args.new_transferable {
                tier.transferable = transferable;
            }
        }

        Ok(())
    }

    // Public mint for regular tiers (Tree/Steel, Bronze, Silver, Gold, Platinum)
    pub fn mint_nft(ctx: Context<MintNft>, args: MintNftArgs) -> Result<()> {
        // Clone AccountInfo before taking a mutable reference
        let global_config_info = ctx.accounts.global_config.to_account_info();
        let global_config = &mut ctx.accounts.global_config;

        // 1) Check global sales pause
        require!(!global_config.is_sales_paused, PresaleError::SalesPaused);

        // 2) Resolve tier by index
        let idx = args.tier_id as usize;
        require!(idx < global_config.tiers.len(), PresaleError::InvalidTierId);

        let tier = &mut global_config.tiers[idx];

        // 3) Supply checks
        require!(tier.supply_minted < tier.supply_total, PresaleError::TierSoldOut);
        require!(tier.price_lamports > 0, PresaleError::TierPriceNotSet);

        // 4) KYC / invite flags (proofs are opaque blobs for now)
        if tier.kyc_required {
            require!(args.kyc_proof.is_some(), PresaleError::KycRequired);
        }

        if tier.invite_only {
            require!(args.invite_proof.is_some(), PresaleError::InviteRequired);
        }

        // 5) Payment in lamports: payer -> admin
        let cpi_ctx_transfer = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.admin.to_account_info(),
            },
        );
        system_program::transfer(cpi_ctx_transfer, tier.price_lamports)?;

        // 6) Mint 1 token (NFT) to payer's associated token account
        let cpi_ctx_mint = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.payer_token_account.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        );
        token::mint_to(cpi_ctx_mint, 1)?;

        // 7) Create Metaplex metadata with placeholder (mystery box)
        let bump = ctx.bumps.global_config;
        let signer_seeds: &[&[u8]] = &[GLOBAL_CONFIG_SEED, &[bump]];
        let signer: &[&[&[u8]]] = &[signer_seeds];

        let data = DataV2 {
            name: PLACEHOLDER_NAME.to_string(),
            symbol: PLACEHOLDER_SYMBOL.to_string(),
            uri: placeholder_uri_for_index(idx).to_string(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                mint_authority: ctx.accounts.payer.to_account_info(),
                update_authority: global_config_info, // <- вот так
                payer: ctx.accounts.payer.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer,
        );

        create_metadata_accounts_v3(
            cpi_ctx,
            data,
            true, // is_mutable
            true, // update_authority_is_signer (PDA signs via seeds)
            None, // collection_details
        )?;

        // 8) Reserve one slot in supply for this mint
        tier.supply_minted += 1;

        Ok(())
    }


    // Special mint for WS-20 (invite-only, soulbound)
    pub fn mint_ws20(_ctx: Context<MintWs20>, _args: MintWs20Args) -> Result<()> {
        // TODO:
        // - verify WS invite proof
        // - enforce WS-20 supply limit and soulbound rules
        Ok(())
    }

    // Admin mint (no payment, used for manual grants)
    pub fn admin_mint(_ctx: Context<AdminMint>, _args: AdminMintArgs) -> Result<()> {
        // TODO:
        // - mint NFT of given tier to target wallet
        Ok(())
    }
}

// ---------------------------------------------
// Tier identifiers (must match frontend JSON)
// ---------------------------------------------
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum TierId {
    TreeSteel = 0,
    Bronze = 1,
    Silver = 2,
    Gold = 3,
    Platinum = 4,
    Ws20 = 5,
}

impl TierId {
    pub fn as_index(self) -> usize {
        self as usize
    }
}

// ---------------------------------------------
// Tier configuration stored on-chain
// ---------------------------------------------
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct TierConfig {
    pub id: u8,               // must equal TierId as integer
    pub supply_total: u16,    // max allowed supply
    pub supply_minted: u16,   // current mint count
    pub price_lamports: u64,  // price in lamports
    pub kyc_required: bool,   // true for Silver+, WS20
    pub invite_only: bool,    // true for WS20
    pub transferable: bool,   // false for WS20 (soulbound)
}

impl TierConfig {
    pub fn for_tier(tier: TierId) -> Self {
        match tier {
            TierId::TreeSteel => Self {
                id: TierId::TreeSteel as u8,
                supply_total: 2000,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: false,
                invite_only: false,
                transferable: true,
            },
            TierId::Bronze => Self {
                id: TierId::Bronze as u8,
                supply_total: 1000,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: false,
                invite_only: false,
                transferable: true,
            },
            TierId::Silver => Self {
                id: TierId::Silver as u8,
                supply_total: 200,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: true,
                invite_only: false,
                transferable: true,
            },
            TierId::Gold => Self {
                id: TierId::Gold as u8,
                supply_total: 100,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: true,
                invite_only: false,
                transferable: true,
            },
            TierId::Platinum => Self {
                id: TierId::Platinum as u8,
                supply_total: 20,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: true,
                invite_only: false,
                transferable: true,
            },
            TierId::Ws20 => Self {
                id: TierId::Ws20 as u8,
                supply_total: 20,
                supply_minted: 0,
                price_lamports: 0,
                kyc_required: true,
                invite_only: true,
                transferable: false,
            },
        }
    }
}

// ---------------------------------------------
// Global configuration (PDA)
// ---------------------------------------------
pub const PLACEHOLDER_NAME: &str = "VIGRI Mystery NFT";
pub const PLACEHOLDER_SYMBOL: &str = "VIGRI";
pub const PLACEHOLDER_URI: &str = "https://example.com/vigri-mystery.json";
pub const GLOBAL_CONFIG_SEED: &[u8] = b"global-config";
pub const GLOBAL_CONFIG_SPACE: usize = 8 + 32 * 3 + 1 + 7 + 6 * 16 + 32;

#[account]
pub struct GlobalConfig {
    pub admin: Pubkey,            // authority of the program
    pub collection_mint: Pubkey,  // main Metaplex collection mint
    pub payment_mint: Pubkey,     // for future SPL payments (v1 can ignore)
    pub is_sales_paused: bool,    // global pause switch
    pub tiers: [TierConfig; 6],   // fixed set of 6 tiers
}

impl GlobalConfig {
    pub fn default_tiers() -> [TierConfig; 6] {
        [
            TierConfig::for_tier(TierId::TreeSteel),
            TierConfig::for_tier(TierId::Bronze),
            TierConfig::for_tier(TierId::Silver),
            TierConfig::for_tier(TierId::Gold),
            TierConfig::for_tier(TierId::Platinum),
            TierConfig::for_tier(TierId::Ws20),
        ]
    }
}

fn placeholder_uri_for_index(idx: usize) -> &'static str {
    match idx {
        0 => "https://example.com/tree-steel.json",
        1 => "https://example.com/bronze.json",
        2 => "https://example.com/silver.json",
        3 => "https://example.com/gold.json",
        4 => "https://example.com/platinum.json",
        5 => "https://example.com/ws20.json",
        _ => "",
    }
}

// ---------------------------------------------
// Instruction argument structs
// ---------------------------------------------

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeArgs {
    pub admin: Pubkey,
    pub collection_mint: Pubkey,
    pub payment_mint: Pubkey, // can be placeholder for native SOL in v1
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateConfigArgs {
    pub is_sales_paused: Option<bool>,
    // Optional per-tier updates: price, flags, etc.
    pub tier_id: Option<u8>,
    pub new_price_lamports: Option<u64>,
    pub new_kyc_required: Option<bool>,
    pub new_invite_only: Option<bool>,
    pub new_transferable: Option<bool>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MintNftArgs {
    pub tier_id: u8,
    // Opaque proof blobs signed off-chain by KYC / invite authority
    pub kyc_proof: Option<Vec<u8>>,
    pub invite_proof: Option<Vec<u8>>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MintWs20Args {
    // WS-20 mint requires a special invite proof
    pub ws_invite_proof: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AdminMintArgs {
    pub tier_id: u8,
    pub recipient: Pubkey,
}

// ---------------------------------------------
// Account context structs (skeletons)
// ---------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: authority to be stored as admin in GlobalConfig
    pub admin: UncheckedAccount<'info>,

    #[account(
        init,
        payer = payer,
        space = GLOBAL_CONFIG_SPACE,
        seeds = [GLOBAL_CONFIG_SEED],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [GLOBAL_CONFIG_SEED],
        bump,
        has_one = admin,
    )]
    pub global_config: Account<'info, GlobalConfig>,
}

#[derive(Accounts)]
pub struct MintNft<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [GLOBAL_CONFIG_SEED],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// CHECK: admin is validated by the `address = global_config.admin` constraint
    #[account(
        mut,
        address = global_config.admin,
    )]
    pub admin: UncheckedAccount<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = 0,
        mint::authority = payer,
        mint::freeze_authority = payer,
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = payer,
    )]
    pub payer_token_account: Account<'info, TokenAccount>,

    /// CHECK: Metaplex metadata account PDA for this mint
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// CHECK: Metaplex Token Metadata program
    pub token_metadata_program: Program<'info, Metadata>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintWs20<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum PresaleError {
    #[msg("Sales are currently paused")]
    SalesPaused,

    #[msg("Invalid tier id")]
    InvalidTierId,

    #[msg("No remaining supply for this tier")]
    TierSoldOut,

    #[msg("Price is not set for this tier")]
    TierPriceNotSet,

    #[msg("KYC is required for this tier")]
    KycRequired,

    #[msg("Invite is required for this tier")]
    InviteRequired,

    #[msg("Only admin can perform this action")]
    Unauthorized,
}
