# NorseCoin Solana Deployment Guide

## Prerequisites

### 1. Install Solana CLI
```bash
# macOS/Linux
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

# Verify
solana --version
```

### 2. Install Anchor CLI
```bash
# Using avm (Anchor Version Manager)
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install 0.30.0
avm use 0.30.0

# Verify
anchor --version
```

### 3. Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 4. Install Node.js Dependencies
```bash
npm install
```

### 5. Create a Solana Wallet
```bash
# Generate a new keypair (for devnet testing)
solana-keygen new -o ~/.config/solana/id.json

# For mainnet, use a hardware wallet or secure key management
```

---

## Devnet Deployment

### Step 1: Configure for Devnet
```bash
solana config set --url devnet
solana airdrop 5  # Get test SOL
```

### Step 2: Build the Program
```bash
anchor build
```

### Step 3: Get the Program ID
After building, get the generated program ID:
```bash
solana-keygen pubkey target/deploy/norse_token-keypair.json
```
Update `declare_id!()` in `programs/norse-token/src/lib.rs` and `Anchor.toml` with this ID.

### Step 4: Rebuild with Correct Program ID
```bash
anchor build
```

### Step 5: Deploy
```bash
anchor deploy --provider.cluster devnet
```

### Step 6: Run the Deployment Script
```bash
npx ts-node app/deploy.ts
```
This will:
1. Create the NORSE SPL token mint
2. Mint 1 trillion tokens to the deployer
3. Initialize the staking pool with Nine Realms
4. Fund the reward pool
5. Initialize the presale with 3 stages
6. Initialize the NFT collection

### Step 7: Run Tests
```bash
anchor test
```

---

## Mainnet Deployment Checklist

- [ ] Audit the program code (strongly recommended)
- [ ] Update `Anchor.toml` cluster to `mainnet`
- [ ] Set `solana config set --url mainnet-beta`
- [ ] Ensure deployer wallet has sufficient SOL (~5 SOL for deployment + rent)
- [ ] Update program ID in `declare_id!()` and `Anchor.toml`
- [ ] Build with `anchor build`
- [ ] Deploy with `anchor deploy --provider.cluster mainnet`
- [ ] Run the deployment script against mainnet
- [ ] Verify all PDAs and accounts are created correctly
- [ ] Transfer ownership/authority as needed
- [ ] Set presale TGE time
- [ ] Enable NFT minting when ready
- [ ] Activate presale stages as scheduled

```bash
# Mainnet deployment
solana config set --url mainnet-beta
anchor deploy --provider.cluster mainnet
```

---

## Key Differences from EVM (BSC) Version

### 1. No Native Transfer Tax
The BSC NorseCoin implements a 10% transfer tax (3% burn, 4% liquidity, 3% dev) inside the ERC-20 `_update` override. Solana's SPL Token standard does not support overriding transfer logic.

**Options for production:**
- **Token-2022 (SPL Token Extensions):** Use the Transfer Fee extension to add a percentage-based fee on every transfer. This is the recommended approach. The fee is collected into withheld accounts and can be harvested by the authority.
- **Custom Transfer Instruction:** Require all transfers to go through a program instruction that deducts tax. This breaks composability with DEXs and wallets.

For the MVP, this program creates a **standard SPL token** without transfer tax.

### 2. Decimals: 9 vs 18
- BSC (ERC-20): 18 decimals (standard for Ethereum/BSC)
- Solana (SPL): 9 decimals (standard for Solana)
- All tier thresholds, staking amounts, and presale calculations are adjusted accordingly

### 3. PDAs Instead of Contract Storage
- BSC: State stored in contract storage slots (mappings, arrays)
- Solana: State stored in Program Derived Addresses (PDAs) -- separate accounts
- Each user's stake is a unique PDA: `[b"user-stake", user_pubkey, realm_id]`
- Global state (staking pool, presale) are singleton PDAs

### 4. SOL for Gas Instead of BNB
- NFT mint price: 0.5 SOL (instead of 0.05 BNB)
- Presale prices: 0.00001 / 0.00002 / 0.00003 SOL per token

### 5. Account Model
- BSC: Single contract holds all state
- Solana: Each piece of state is a separate account with rent-exempt balance
- Users pay rent for account creation (refundable when account is closed)

### 6. NFTs
- BSC: Custom ERC-721 contract
- Solana: SPL Token mints with supply of 1, compatible with Metaplex metadata standard
- For full Metaplex integration, add the `mpl-token-metadata` crate and create metadata accounts

### 7. Concurrency
- BSC: Sequential transaction execution per block
- Solana: Parallel execution -- transactions touching different accounts can run simultaneously
- This means multiple users can stake in different realms without blocking each other

---

## Cost Estimates

| Operation | BSC (BNB) | Solana (SOL) |
|-----------|-----------|--------------|
| Program deploy | ~0.05 BNB ($15) | ~3 SOL ($0.30) |
| Token creation | ~0.005 BNB ($1.50) | ~0.002 SOL ($0.0002) |
| Staking tx | ~0.002 BNB ($0.60) | ~0.000005 SOL ($0.0005) |
| NFT mint | ~0.003 BNB ($0.90) | ~0.01 SOL ($0.001) |
| Presale buy | ~0.002 BNB ($0.60) | ~0.000005 SOL ($0.0005) |

*Solana transaction fees are roughly 1000x cheaper than BSC.*

---

## Program Accounts Reference

| Account | PDA Seeds | Description |
|---------|-----------|-------------|
| Mint Authority | `["mint-authority"]` | Controls token minting |
| Staking Pool | `["staking-pool"]` | Global staking state + realm configs |
| Staking Vault | `["staking-vault"]` | Token account holding staked tokens |
| User Stake | `["user-stake", user, realm_id]` | Per-user per-realm stake info |
| NFT Collection | `["nft-collection"]` | NFT collection metadata |
| User NFT | `["user-nft", user]` | Per-user NFT mint count |
| Presale State | `["presale-state"]` | Presale config + stage data |
| Presale Token Vault | `["presale-token-vault"]` | Holds tokens for presale claims |
| Presale SOL Vault | `["presale-vault"]` | Holds SOL from presale purchases |
| User Presale | `["user-presale", user]` | Per-user presale purchase + vesting |

---

## Upgradeability

By default, Anchor programs are upgradeable (the deployer keypair is the upgrade authority). For production:

```bash
# Transfer upgrade authority to a multisig
solana program set-upgrade-authority <PROGRAM_ID> --new-upgrade-authority <MULTISIG_ADDRESS>

# Or make the program immutable (irreversible!)
solana program set-upgrade-authority <PROGRAM_ID> --final
```
