#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
    metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata},
};

// DataV2 re-exported via anchor_spl::metadata
use anchor_spl::metadata::mpl_token_metadata::types::{DataV2, Creator};


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

        // serial внутри tier: minted + 1 (до инкремента)
        let global_config = &mut ctx.accounts.global_config;
        let tier_idx = args.tier_id as usize;
        let tier = &mut global_config.tiers[tier_idx];
        let serial: u16 = tier.supply_minted + 1;

        // Emit event with tier, serial, and computed design key
        let design_key = resolve_design_key(args.tier_id, serial, args.design_choice)?;
        emit!(NftMinted {
            tier_id: args.tier_id,
            serial,
            design_key,
            mint: ctx.accounts.mint.key(),
        });

        let data = DataV2 {
            name: PLACEHOLDER_NAME.to_string(),
            symbol: PLACEHOLDER_SYMBOL.to_string(),
            uri: build_uri(args.tier_id, serial, args.design_choice)?,
            seller_fee_basis_points: 250,
            creators: Some(vec![Creator {
                address: ctx.accounts.admin.key(),
                verified: false,
                share: 100,
            }]),
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
    pub fn admin_mint(ctx: Context<AdminMint>, args: AdminMintArgs) -> Result<()> {
        // 1) Load global config and resolve tier
        let global_config_info = ctx.accounts.global_config.to_account_info();
        let global_config = &mut ctx.accounts.global_config;

        let idx = args.tier_id as usize;
        require!(idx < global_config.tiers.len(), PresaleError::InvalidTierId);

        let tier = &mut global_config.tiers[idx];

        // 2) Supply limits
        require!(tier.supply_minted < tier.supply_total, PresaleError::TierSoldOut);

        let is_ws20 = tier.id == TierId::Ws20 as u8;
        if !is_ws20 {
            // Non-WS20 tiers: max 5% for admin mint
            let max_admin = tier.supply_total / 20; // 5%
            if max_admin > 0 {
                require!(tier.admin_minted < max_admin, PresaleError::TierSoldOut);
            }
        }
        // For WS20 we only enforce total supply limit above.

        // 3) Mint 1 NFT to admin (treasury = admin)
        let cpi_ctx_mint = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.admin_token_account.to_account_info(),
                authority: ctx.accounts.admin.to_account_info(),
            },
        );
        token::mint_to(cpi_ctx_mint, 1)?;

        // 4) Create Metaplex metadata with the same placeholder as public mint
        let bump = ctx.bumps.global_config;
        let signer_seeds: &[&[u8]] = &[GLOBAL_CONFIG_SEED, &[bump]];
        let signer: &[&[&[u8]]] = &[signer_seeds];

        let global_config = &mut ctx.accounts.global_config;
        let tier_idx = args.tier_id as usize;
        let tier = &mut global_config.tiers[tier_idx];
        let serial: u16 = tier.supply_minted + 1;

        // Emit event with tier, serial, and computed design key
        let design_key = resolve_design_key(args.tier_id, serial, args.design_choice)?;
        emit!(NftMinted {
            tier_id: args.tier_id,
            serial,
            design_key,
            mint: ctx.accounts.mint.key(),
        });

        let data = DataV2 {
            name: PLACEHOLDER_NAME.to_string(),
            symbol: PLACEHOLDER_SYMBOL.to_string(),
            uri: build_uri(args.tier_id, serial, args.design_choice)?,
            seller_fee_basis_points: 250,
            creators: Some(vec![Creator {
                address: ctx.accounts.admin.key(),
                verified: false,
                share: 100,
            }]),
            collection: None,
            uses: None,
        };

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                mint_authority: ctx.accounts.admin.to_account_info(),
                update_authority: global_config_info,
                payer: ctx.accounts.admin.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer,
        );

        create_metadata_accounts_v3(
            cpi_ctx,
            data,
            true, // is_mutable
            true, // update_authority_is_signer (PDA global_config signs)
            None,
        )?;

        // 5) Update counters
        tier.supply_minted += 1;
        tier.admin_minted += 1;

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
    pub admin_minted: u16,    // minted via admin_mint
    pub price_lamports: u64,  // price in lamports
    pub kyc_required: bool,   // true for Silver+, WS20
    pub invite_only: bool,    // true for WS20
    pub transferable: bool,   // false for WS20 (soulbound)
    pub reserved: [u8; 8],    // future flags / counters (do not touch now)
}

impl TierConfig {
    pub fn for_tier(tier: TierId) -> Self {
        match tier {
            TierId::TreeSteel => Self {
                id: TierId::TreeSteel as u8,
                supply_total: 2000,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 500_000_000, // 0.5 SOL
                kyc_required: false,
                invite_only: false,
                transferable: true,
                reserved: [0; 8],
            },
            TierId::Bronze => Self {
                id: TierId::Bronze as u8,
                supply_total: 1000,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 2_000_000_000, // 2 SOL
                kyc_required: false,
                invite_only: false,
                transferable: true,
                reserved: [0; 8],
            },
            TierId::Silver => Self {
                id: TierId::Silver as u8,
                supply_total: 200,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 10_000_000_000, // 10 SOL
                kyc_required: true,
                invite_only: false,
                transferable: true,
                reserved: [0; 8],
            },
            TierId::Gold => Self {
                id: TierId::Gold as u8,
                supply_total: 100,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 40_000_000_000, // 40 SOL
                kyc_required: true,
                invite_only: false,
                transferable: true,
                reserved: [0; 8],
            },
            TierId::Platinum => Self {
                id: TierId::Platinum as u8,
                supply_total: 20,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 80_000_000_000, // 80 SOL
                kyc_required: true,
                invite_only: false,
                transferable: true,
                reserved: [0; 8],
            },
            TierId::Ws20 => Self {
                id: TierId::Ws20 as u8,
                supply_total: 20,
                supply_minted: 0,
                admin_minted: 0,
                price_lamports: 0, // 0 SOL
                kyc_required: true,
                invite_only: true,
                transferable: false,
                reserved: [0; 8],
            },
        }
    }
}

// ---------------------------------------------
// Global configuration (PDA)
// ---------------------------------------------
pub const PLACEHOLDER_NAME: &str = "VIGRI Mystery NFT";
pub const PLACEHOLDER_SYMBOL: &str = "VIGRINFT";
pub const PLACEHOLDER_URI: &str = "https://vigri.ee/metadata/nft/vigri-mystery.json";
// Final PDA seed for the presale global config
pub const GLOBAL_CONFIG_SEED: &[u8] = b"vigri-presale-config";

// Generous space for GlobalConfig + padding + reserved
pub const GLOBAL_CONFIG_SPACE: usize = 8 + 512;

#[account]
pub struct GlobalConfig {
    pub admin: Pubkey,            // authority of the program
    pub collection_mint: Pubkey,  // main Metaplex collection mint
    pub payment_mint: Pubkey,     // for future SPL payments (v1 can ignore)
    pub is_sales_paused: bool,    // global pause switch
    pub tiers: [TierConfig; 6],   // fixed set of 6 tiers
    pub reserved: [u8; 64],       // future use, keep zeroed
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

fn build_uri(tier_id: u8, serial: u16, design_choice: Option<u8>) -> Result<String> {
    let serial6 = format!("{:06}", serial);

    let uri = match tier_id {
        // 0 = Tree/Steel (choice)
        0 => {
            let code = match design_choice {
                Some(1) => "TR",
                Some(2) => "FE",
                _ => return err!(PresaleError::InvalidDesignChoice),
            };
            format!("https://vigri.ee/metadata/nft/tree-steel/{}/{}.json", code, serial6)
        }

        // 1 = Bronze -> CU
        1 => format!("https://vigri.ee/metadata/nft/bronze/CU/{}.json", serial6),

        // 2 = Silver -> AG
        // Variant (DesignKey 1..10) now computed off-chain from serial,
        // URI no longer encodes vXX.
        2 => format!(
            "https://vigri.ee/metadata/nft/silver/AG/{}.json",
            serial6
        ),

        // 3 = Gold -> AU
        3 => format!("https://vigri.ee/metadata/nft/gold/AU/{}.json", serial6),

        // 4 = Platinum -> PT
        4 => format!("https://vigri.ee/metadata/nft/platinum/PT/{}.json", serial6),

        // 5 = WS20 -> WS
        5 => format!("https://vigri.ee/metadata/nft/ws/WS/{}.json", serial6),

        _ => return err!(PresaleError::InvalidTierId),
    };

    Ok(uri)
}

#[event]
pub struct NftMinted {
    pub tier_id: u8,
    pub serial: u16,
    pub design_key: u16,
    pub mint: Pubkey,
}

fn resolve_design_key(tier_id: u8, serial: u16, design_choice: Option<u8>) -> Result<u16> {
    match tier_id {
        0 => match design_choice {
            Some(1) => Ok(1), // TR
            Some(2) => Ok(2), // FE
            _ => err!(PresaleError::InvalidDesignChoice),
        },
        1 => Ok(1), // CU
        2 => Ok(((serial - 1) % 10) + 1), // AG: 1..10
        3 => Ok(serial), // AU
        4 => Ok(serial), // PT
        5 => Ok(serial), // WS
        _ => err!(PresaleError::InvalidTierId),
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
    // Only used for tier_id == 0 (Tree/Steel):
    // 1 = TR (Tree), 2 = FE (Steel)
    pub design_choice: Option<u8>,

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

    // Only used for tier_id == 0 (Tree/Steel):
    // 1 = TR (Tree), 2 = FE (Steel)
    pub design_choice: Option<u8>,
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
    /// Admin = authority of the program and treasury owner
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [GLOBAL_CONFIG_SEED],
        bump,
        has_one = admin,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 0,
        mint::authority = admin,
        mint::freeze_authority = admin,
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

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

    #[msg("Invalid design choice for this tier")]
    InvalidDesignChoice,
}
