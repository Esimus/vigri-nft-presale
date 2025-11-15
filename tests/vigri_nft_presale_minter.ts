// @ts-nocheck

import * as anchor from "@coral-xyz/anchor";
import { VigriNftPresaleMinter } from "../target/types/vigri_nft_presale_minter";

describe("vigri_nft_presale_minter", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .vigriNftPresaleMinter as anchor.Program<VigriNftPresaleMinter>;

  it("initializes global config", async () => {
    const admin = provider.wallet.publicKey;

    // 1) Inspect what the IDL expects for `initialize`
    const initIx = program.idl.instructions.find(
      (ix) => ix.name === "initialize"
    );
    console.log("IDL initialize instruction:", JSON.stringify(initIx, null, 2));

    // 2) Prepare the accounts we pass to the instruction
    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("global-config")], // seed must match GLOBAL_CONFIG_SEED in Rust
      program.programId
    );

    const accounts = {
      payer: provider.wallet.publicKey,
      admin,
      globalConfig: globalConfigPda,
      systemProgram: anchor.web3.SystemProgram.programId,
    };

    console.log("Accounts we pass:", {
      payer: accounts.payer.toBase58(),
      admin: accounts.admin.toBase58(),
      globalConfig: accounts.globalConfig.toBase58(),
      systemProgram: accounts.systemProgram.toBase58(),
    });

    // 3) Call `initialize` with args and accounts
    const args = {
      admin,
      collectionMint: admin, // temporary placeholders, just valid Pubkeys
      paymentMint: admin,
    };

    const tx = await program.methods
      .initialize(args)
      .accounts(accounts)
      .rpc();

    console.log("initialize tx:", tx);
  });
});
