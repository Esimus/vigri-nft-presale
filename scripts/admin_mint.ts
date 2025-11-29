// scripts/admin_mint.ts
// Simple CLI tool to call admin_mint from your local machine.
// Usage (devnet):
//   yarn admin-mint <tierId> <count>
// Example:
//   yarn admin-mint 5 20   // WS-20, 20 NFTs
//   yarn admin-mint 4 1    // Platinum, 1 NFT

import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const idl = require("../target/idl/vigri_nft_presale_minter.json");

const PROGRAM_ID = new PublicKey(
  "GmrUAwBvC3ijaM2L7kjddQFMWHevxRnArngf7jFx1yEk"
);

// Well-known program IDs
const TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
);
const TOKEN_METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

// PDA seed must match GLOBAL_CONFIG_SEED in Rust
const GLOBAL_CONFIG_SEED = "vigri-presale-config";

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const wallet = provider.wallet.publicKey;
  console.log("Admin wallet (from Solana CLI):", wallet.toBase58());

  const [globalConfigPda] = PublicKey.findProgramAddressSync(
    [Buffer.from(GLOBAL_CONFIG_SEED)],
    PROGRAM_ID
  );
  console.log("GlobalConfig PDA:", globalConfigPda.toBase58());

  // Parse CLI args
  const tierId = parseInt(process.argv[2] ?? "0", 10);
  const count = parseInt(process.argv[3] ?? "1", 10);

  if (Number.isNaN(tierId) || Number.isNaN(count) || count <= 0) {
    console.log("Usage: yarn admin-mint <tierId> <count>");
    process.exit(1);
  }

  console.log(`admin_mint tierId=${tierId}, count=${count}`);

  const program = new anchor.Program(
    idl as anchor.Idl,
    provider,
  );

  for (let i = 0; i < count; i++) {
    const mintKeypair = anchor.web3.Keypair.generate();

    const [adminTokenAccount] = PublicKey.findProgramAddressSync(
      [
        wallet.toBuffer(),
        TOKEN_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    const [metadataPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        TOKEN_METADATA_PROGRAM_ID.toBuffer(),
        mintKeypair.publicKey.toBuffer(),
      ],
      TOKEN_METADATA_PROGRAM_ID
    );

    const adminMintArgs = {
      tierId,
      recipient: wallet,
    };

    const adminMintAccounts = {
      admin: wallet,
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

    console.log(`\n[${i + 1}/${count}] admin_mint...`);
    console.log("mint:", mintKeypair.publicKey.toBase58());
    console.log("adminTokenAccount:", adminTokenAccount.toBase58());
    console.log("metadata:", metadataPda.toBase58());

    const tx = await program.methods
      .adminMint(adminMintArgs)
      .accounts(adminMintAccounts)
      .signers([mintKeypair])
      .rpc();

    console.log("tx:", tx);
  }

  console.log("\nDone.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
