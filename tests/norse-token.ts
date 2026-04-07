import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import { expect } from "chai";

// Import the generated IDL type
import { NorseToken } from "../target/types/norse_token";

describe("norse-token", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.NorseToken as Program<NorseToken>;
  const deployer = provider.wallet;

  let tokenMint: Keypair;
  let mintAuthority: PublicKey;
  let mintAuthorityBump: number;
  let deployerTokenAccount: PublicKey;
  let stakingPool: PublicKey;
  let stakingVault: PublicKey;
  let presaleState: PublicKey;
  let presaleTokenVault: PublicKey;
  let nftCollection: PublicKey;

  before(async () => {
    tokenMint = Keypair.generate();

    [mintAuthority, mintAuthorityBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint-authority")],
      program.programId
    );

    deployerTokenAccount = await getAssociatedTokenAddress(
      tokenMint.publicKey,
      deployer.publicKey
    );

    [stakingPool] = PublicKey.findProgramAddressSync(
      [Buffer.from("staking-pool")],
      program.programId
    );

    [stakingVault] = PublicKey.findProgramAddressSync(
      [Buffer.from("staking-vault")],
      program.programId
    );

    [presaleState] = PublicKey.findProgramAddressSync(
      [Buffer.from("presale-state")],
      program.programId
    );

    [presaleTokenVault] = PublicKey.findProgramAddressSync(
      [Buffer.from("presale-token-vault")],
      program.programId
    );

    [nftCollection] = PublicKey.findProgramAddressSync(
      [Buffer.from("nft-collection")],
      program.programId
    );
  });

  // ────────────────────────────────────────────────
  //  Token Initialization
  // ────────────────────────────────────────────────

  it("initializes the NORSE token with 1 trillion supply", async () => {
    await program.methods
      .initializeToken()
      .accounts({
        deployer: deployer.publicKey,
        tokenMint: tokenMint.publicKey,
        mintAuthority,
        deployerTokenAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([tokenMint])
      .rpc();

    // Verify token account balance
    const balance = await provider.connection.getTokenAccountBalance(
      deployerTokenAccount
    );

    // 1 trillion tokens with 9 decimals = 1e21 smallest units
    expect(balance.value.uiAmount).to.equal(1_000_000_000_000);
    expect(Number(balance.value.decimals)).to.equal(9);

    console.log(
      `  Token mint created: ${tokenMint.publicKey.toBase58()}`
    );
    console.log(`  Deployer balance: ${balance.value.uiAmountString} NORSE`);
  });

  // ────────────────────────────────────────────────
  //  Viking Tier
  // ────────────────────────────────────────────────

  it("returns correct Viking tier for deployer", async () => {
    // Deployer holds 1 trillion = Konungr (tier 5)
    const tier = await program.methods
      .getVikingTier()
      .accounts({
        userTokenAccount: deployerTokenAccount,
      })
      .view();

    expect(tier).to.equal(5); // Konungr
    console.log(`  Deployer Viking tier: ${tier} (Konungr)`);
  });

  // ────────────────────────────────────────────────
  //  Staking Initialization
  // ────────────────────────────────────────────────

  it("initializes the staking pool with Nine Realms", async () => {
    await program.methods
      .initializeStaking()
      .accounts({
        authority: deployer.publicKey,
        tokenMint: tokenMint.publicKey,
        stakingPool,
        stakingVault,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const pool = await program.account.stakingPool.fetch(stakingPool);
    expect(pool.authority.toBase58()).to.equal(deployer.publicKey.toBase58());
    expect(pool.totalStaked.toNumber()).to.equal(0);
    expect(pool.realms.length).to.equal(9);

    // Verify realm configs
    const realmNames = [
      "Midgard",
      "Asgard",
      "Vanaheim",
      "Alfheim",
      "Svartalfheim",
      "Nidavellir",
      "Jotunheim",
      "Niflheim",
      "Muspelheim",
    ];
    const lockDays = [7, 14, 30, 60, 90, 120, 180, 270, 365];
    const apyBps = [6000, 8000, 10000, 15000, 20000, 25000, 30000, 40000, 50000];

    for (let i = 0; i < 9; i++) {
      expect(pool.realms[i].lockDays).to.equal(lockDays[i]);
      expect(pool.realms[i].apyBps).to.equal(apyBps[i]);
      expect(pool.realms[i].enabled).to.be.true;
    }

    console.log(`  Staking pool initialized with 9 realms`);
  });

  // ────────────────────────────────────────────────
  //  Staking Flow
  // ────────────────────────────────────────────────

  it("funds the reward pool", async () => {
    const fundAmount = new anchor.BN(1_000_000).mul(new anchor.BN(1_000_000_000));

    await program.methods
      .fundRewardPool(fundAmount)
      .accounts({
        funder: deployer.publicKey,
        stakingPool,
        stakingVault,
        funderTokenAccount: deployerTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const pool = await program.account.stakingPool.fetch(stakingPool);
    expect(pool.rewardPool.toString()).to.equal(fundAmount.toString());
    console.log(`  Reward pool funded with 1,000,000 NORSE`);
  });

  it("stakes tokens in Midgard (realm 0)", async () => {
    const realmId = 0;
    const stakeAmount = new anchor.BN(1_000).mul(new anchor.BN(1_000_000_000));

    const [userStake] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("user-stake"),
        deployer.publicKey.toBuffer(),
        Buffer.from([realmId]),
      ],
      program.programId
    );

    await program.methods
      .stake(realmId, stakeAmount)
      .accounts({
        user: deployer.publicKey,
        stakingPool,
        stakingVault,
        userStake,
        userTokenAccount: deployerTokenAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const stake = await program.account.userStake.fetch(userStake);
    expect(stake.amount.toString()).to.equal(stakeAmount.toString());
    expect(stake.realmId).to.equal(realmId);
    expect(stake.active).to.be.true;

    const pool = await program.account.stakingPool.fetch(stakingPool);
    expect(pool.totalStaked.toString()).to.equal(stakeAmount.toString());

    console.log(`  Staked 1,000 NORSE in Midgard`);
  });

  // ────────────────────────────────────────────────
  //  Presale Initialization
  // ────────────────────────────────────────────────

  it("initializes the presale", async () => {
    await program.methods
      .initializePresale()
      .accounts({
        authority: deployer.publicKey,
        tokenMint: tokenMint.publicKey,
        presaleState,
        presaleTokenVault,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const presale = await program.account.presaleState.fetch(presaleState);
    expect(presale.authority.toBase58()).to.equal(
      deployer.publicKey.toBase58()
    );
    expect(presale.currentStage).to.equal(0);
    expect(presale.stages.length).to.equal(3);

    // Verify stage prices
    expect(presale.stages[0].priceLamportsPerToken.toNumber()).to.equal(10_000);
    expect(presale.stages[1].priceLamportsPerToken.toNumber()).to.equal(20_000);
    expect(presale.stages[2].priceLamportsPerToken.toNumber()).to.equal(30_000);

    // All stages start inactive
    for (let i = 0; i < 3; i++) {
      expect(presale.stages[i].active).to.be.false;
    }

    console.log(`  Presale initialized with 3 stages`);
  });

  it("activates presale stage 0 (Ragnarok)", async () => {
    await program.methods
      .setStageActive(0, true)
      .accounts({
        presaleState,
        authority: deployer.publicKey,
      })
      .rpc();

    const presale = await program.account.presaleState.fetch(presaleState);
    expect(presale.stages[0].active).to.be.true;
    expect(presale.currentStage).to.equal(0);

    console.log(`  Presale stage 0 (Ragnarok) activated`);
  });

  it("buys tokens in presale", async () => {
    // Buy with 0.1 SOL = 100,000,000 lamports
    const solAmount = new anchor.BN(100_000_000);

    const [userPresale] = PublicKey.findProgramAddressSync(
      [Buffer.from("user-presale"), deployer.publicKey.toBuffer()],
      program.programId
    );

    const [presaleVault] = PublicKey.findProgramAddressSync(
      [Buffer.from("presale-vault")],
      program.programId
    );

    await program.methods
      .buyPresale(solAmount)
      .accounts({
        buyer: deployer.publicKey,
        presaleState,
        userPresale,
        presaleVault,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const userPresaleAccount = await program.account.userPresale.fetch(
      userPresale
    );

    // 100,000,000 lamports / 10,000 lamports per token * 1e9 = 10,000 * 1e9
    const expectedTokens = 100_000_000 / 10_000 * 1_000_000_000;
    expect(userPresaleAccount.totalPurchased.toNumber()).to.equal(expectedTokens);

    console.log(
      `  Purchased ${expectedTokens / 1_000_000_000} NORSE tokens for 0.1 SOL`
    );
  });

  // ────────────────────────────────────────────────
  //  NFT Collection
  // ────────────────────────────────────────────────

  it("initializes the NFT collection", async () => {
    await program.methods
      .initializeNftCollection()
      .accounts({
        authority: deployer.publicKey,
        nftCollection,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const collection = await program.account.nftCollection.fetch(nftCollection);
    expect(collection.authority.toBase58()).to.equal(
      deployer.publicKey.toBase58()
    );
    expect(collection.totalMinted.toNumber()).to.equal(0);
    expect(collection.mintPrice.toNumber()).to.equal(500_000_000); // 0.5 SOL
    expect(collection.mintingEnabled).to.be.false;

    console.log(`  NFT collection initialized`);
  });
});
