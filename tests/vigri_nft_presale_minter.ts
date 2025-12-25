// @ts-nocheck

import * as anchor from "@coral-xyz/anchor";
import { VigriNftPresaleMinter } from "../target/types/vigri_nft_presale_minter";

describe("vigri_nft_presale_minter", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .vigriNftPresaleMinter as anchor.Program<VigriNftPresaleMinter>;

  it("initializes config if needed, sets tier prices and mints one NFT", async () => {
    const admin = provider.wallet.publicKey;

    // GlobalConfig PDA (must match GLOBAL_CONFIG_SEED in Rust)
    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vigri-presale-config")],
      program.programId
    );

    // 1) Initialize GlobalConfig only if it does not exist yet
    const existingGlobalConfig = await provider.connection.getAccountInfo(
      globalConfigPda
    );

    if (!existingGlobalConfig) {
      const initAccounts = {
        payer: provider.wallet.publicKey,
        admin,
        globalConfig: globalConfigPda,
        systemProgram: anchor.web3.SystemProgram.programId,
      };

      const initArgs = {
        admin,
        collectionMint: admin, // temporary placeholder pubkeys
        paymentMint: admin,
      };

      const initTx = await program.methods
        .initialize(initArgs)
        .accounts(initAccounts)
        .rpc();

      console.log("initialize tx:", initTx);
    } else {
      console.log(
        "GlobalConfig already exists, skipping initialize. PDA:",
        globalConfigPda.toBase58()
      );
    }

    // 2) Set non-zero prices for tiers 0–4 via update_config

    const updateAccounts = {
      admin,
      globalConfig: globalConfigPda,
    };

    const tierPriceConfigs = [
      { tierId: 0, priceLamports: new anchor.BN(500_000_000) },    // 0.5 SOL
      { tierId: 1, priceLamports: new anchor.BN(2_000_000_000) },  // 2 SOL
      { tierId: 2, priceLamports: new anchor.BN(10_000_000_000) }, // 10 SOL
      { tierId: 3, priceLamports: new anchor.BN(40_000_000_000) }, // 40 SOL
      { tierId: 4, priceLamports: new anchor.BN(80_000_000_000) }, // 80 SOL
      // tier 5 (WS-20) remains 0 SOL
    ];

    for (const cfg of tierPriceConfigs) {
      const updateArgs = {
        isSalesPaused: null,
        tierId: cfg.tierId,
        newPriceLamports: cfg.priceLamports,
        newKycRequired: null,
        newInviteOnly: null,
        newTransferable: null,
      };

      const updateTx = await program.methods
        .updateConfig(updateArgs)
        .accounts(updateAccounts)
        .rpc();

      console.log(
        `update_config tier ${cfg.tierId} tx:`,
        updateTx
      );
    }

    // 3) Now mint one NFT for tier 0

    const payer = provider.wallet.publicKey;

    // Well-known program IDs on Solana
    const TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
    );
    const ASSOCIATED_TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
    );
    const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
      "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
    );

    // New mint for the NFT
    const mintKeypair = anchor.web3.Keypair.generate();

    // Payer's ATA for this mint
    const [payerTokenAccount] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        payer.toBuffer(),
        TOKEN_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    // Metadata PDA for this mint
    const [metadataPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        TOKEN_METADATA_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      TOKEN_METADATA_PROGRAM_ID
    );

    const mintArgs = {
      tierId: 0,
      kycProof: null,
      inviteProof: null,
    };

    const mintAccounts = {
      payer,
      globalConfig: globalConfigPda,
      admin,
      mint: mintKeypair.publicKey,
      payerTokenAccount,
      metadata: metadataPda,
      tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
      rent: anchor.web3.SYSVAR_RENT_PUBKEY,
    };

    console.log("Mint accounts:", {
      payer: mintAccounts.payer.toBase58(),
      globalConfig: mintAccounts.globalConfig.toBase58(),
      admin: mintAccounts.admin.toBase58(),
      mint: mintAccounts.mint.toBase58(),
      payerTokenAccount: mintAccounts.payerTokenAccount.toBase58(),
      metadata: mintAccounts.metadata.toBase58(),
    });

    const mintTx = await program.methods
      .mintNft(mintArgs)
      .accounts(mintAccounts)
      .signers([mintKeypair])
      .rpc();

    console.log("mint_nft tx:", mintTx);
  });

  it("admin mints one NFT into treasury for tier 0", async () => {
    const admin = provider.wallet.publicKey;

    // GlobalConfig PDA (must match GLOBAL_CONFIG_SEED in Rust)
    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vigri-presale-config")],
      program.programId
    );

    // Ensure GlobalConfig exists (in case this test runs standalone)
    const existingGlobalConfig = await provider.connection.getAccountInfo(
      globalConfigPda
    );

    if (!existingGlobalConfig) {
      const initAccounts = {
        payer: provider.wallet.publicKey,
        admin,
        globalConfig: globalConfigPda,
        systemProgram: anchor.web3.SystemProgram.programId,
      };

      const initArgs = {
        admin,
        collectionMint: admin, // temporary placeholder pubkeys
        paymentMint: admin,
      };

      const initTx = await program.methods
        .initialize(initArgs)
        .accounts(initAccounts)
        .rpc();

      console.log("initialize (admin_mint test) tx:", initTx);
    }

    // Well-known program IDs on Solana
    const TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
    );
    const ASSOCIATED_TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
    );
    const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
      "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
    );

    // New mint for the NFT
    const mintKeypair = anchor.web3.Keypair.generate();

    // Admin's ATA (treasury) for this mint
    const [adminTokenAccount] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        admin.toBuffer(),
        TOKEN_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    // Metadata PDA for this mint
    const [metadataPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        TOKEN_METADATA_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      TOKEN_METADATA_PROGRAM_ID
    );

    const adminMintArgs = {
      tierId: 0,
    };

    const adminMintAccounts = {
      admin,
      globalConfig: globalConfigPda,
      mint: mintKeypair.publicKey,
      adminTokenAccount,
      metadata: metadataPda,
      tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
      rent: anchor.web3.SYSVAR_RENT_PUBKEY,
    };

    console.log("AdminMint accounts:", {
      admin: adminMintAccounts.admin.toBase58(),
      globalConfig: adminMintAccounts.globalConfig.toBase58(),
      mint: adminMintAccounts.mint.toBase58(),
      adminTokenAccount: adminMintAccounts.adminTokenAccount.toBase58(),
      metadata: adminMintAccounts.metadata.toBase58(),
    });

    const adminMintTx = await program.methods
      .adminMint(adminMintArgs)
      .accounts(adminMintAccounts)
      .signers([mintKeypair])
      .rpc();

    console.log("admin_mint tx:", adminMintTx);
  });

  it("enforces 5% admin mint limit for Platinum", async () => {
    const admin = provider.wallet.publicKey;

    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vigri-presale-config")],
      program.programId
    );

    // Platinum tier (id = 4, supply_total = 20, max admin mint = 1)
    const platinumTierId = 4;

    const TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
    );
    const ASSOCIATED_TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
      "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
    );
    const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
      "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
    );

    const doAdminMintOnce = async () => {
      const mintKeypair = anchor.web3.Keypair.generate();

      const [adminTokenAccount] =
        anchor.web3.PublicKey.findProgramAddressSync(
          [
            admin.toBuffer(),
            TOKEN_PROGRAM_ID.toBuffer(),
            mintKeypair.publicKey.toBuffer(),
          ],
          ASSOCIATED_TOKEN_PROGRAM_ID
        );

      const [metadataPda] = anchor.web3.PublicKey.findProgramAddressSync(
        [
          Buffer.from("metadata"),
          TOKEN_METADATA_PROGRAM_ID.toBuffer(),
          mintKeypair.publicKey.toBuffer(),
        ],
        TOKEN_METADATA_PROGRAM_ID
      );

      const adminMintArgs = {
        tierId: platinumTierId,
        recipient: admin,
      };

      const adminMintAccounts = {
        admin,
        globalConfig: globalConfigPda,
        mint: mintKeypair.publicKey,
        adminTokenAccount,
        metadata: metadataPda,
        tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      };

      const tx = await program.methods
        .adminMint(adminMintArgs)
        .accounts(adminMintAccounts)
        .signers([mintKeypair])
        .rpc();

      console.log("admin_mint Platinum tx:", tx);
    };

    // First admin_mint for Platinum - must pass
    await doAdminMintOnce();

    // The second admin_mint for Platinum should fall under the 5% limit
    try {
      await doAdminMintOnce();
      throw new Error(
        "Second admin_mint for Platinum unexpectedly succeeded (limit 5% not enforced)"
      );
    } catch (err) {
      console.log(
        "Second admin_mint for Platinum failed as expected (5% limit enforced)"
      );
    }
  });
});
