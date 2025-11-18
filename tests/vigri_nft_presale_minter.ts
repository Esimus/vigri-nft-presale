// @ts-nocheck

import * as anchor from "@coral-xyz/anchor";
import { VigriNftPresaleMinter } from "../target/types/vigri_nft_presale_minter";

describe("vigri_nft_presale_minter", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .vigriNftPresaleMinter as anchor.Program<VigriNftPresaleMinter>;

  it("initializes config if needed, sets tier price and mints one NFT", async () => {
    const admin = provider.wallet.publicKey;

    // GlobalConfig PDA (must match GLOBAL_CONFIG_SEED in Rust)
    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("global-config")],
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

    // 2) Set a non-zero price for tier 0 via update_config
    const updateAccounts = {
      admin,
      globalConfig: globalConfigPda,
    };

    const updateArgs = {
      isSalesPaused: null,
      tierId: 0, // TreeSteel tier
      newPriceLamports: new anchor.BN(100_000_000), // 0.1 SOL for tests
      newKycRequired: null,
      newInviteOnly: null,
      newTransferable: null,
    };

    const updateTx = await program.methods
      .updateConfig(updateArgs)
      .accounts(updateAccounts)
      .rpc();

    console.log("update_config tx:", updateTx);

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
});
