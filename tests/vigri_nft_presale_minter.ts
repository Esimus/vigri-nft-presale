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

    // GlobalConfig PDA (must match GLOBAL_CONFIG_SEED in Rust)
    const [globalConfigPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("global-config")],
      program.programId
    );

    const accounts = {
      payer: provider.wallet.publicKey,
      admin,
      globalConfig: globalConfigPda,
      systemProgram: anchor.web3.SystemProgram.programId,
    };

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
