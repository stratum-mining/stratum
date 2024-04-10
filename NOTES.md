<!-- TOC start (generated with https://github.com/derlin/bitdowntoc) -->

- [Thunder Pool ](#thunder-pool)
   * [Overview](#overview)
      + [Database ](#database)
         - [Accounts ](#accounts)
         - [Workers and Shares ](#workers-and-shares)
         - [Payment Scheme](#payment-scheme)
            * [How do we relate the account with the PaymentIntervals ?](#how-do-we-relate-the-account-with-the-paymentintervals-)
            * [How do we check the payment eligibility for an account ?](#how-do-we-check-the-payment-eligibility-for-an-account-)
               + [Using the Payment Scheme](#using-the-payment-scheme)
               + [Not using the Payment Scheme:](#not-using-the-payment-scheme)
   * [Architecture Overview ](#architecture-overview)
      + [Stratum Protocol Implementation v1/v2](#stratum-protocol-implementation-v1v2)
      + [Template Provider](#template-provider)
      + [Mining Pool and Translator Proxy](#mining-pool-and-translator-proxy)
      + [Job Declarator server/client](#job-declarator-serverclient)
   * [Idea box](#idea-box)
      + [Making Merkle Trees Work for Us:](#making-merkle-trees-work-for-us)
         - [Building the Tree:](#building-the-tree)
         - [Validating Contributions:](#validating-contributions)
         - [Direct Blockchain Integration for Balance Management](#direct-blockchain-integration-for-balance-management)
            * [Advantages of Direct Blockchain Balance Management](#advantages-of-direct-blockchain-balance-management)
            * [Using a Blockchain for Account and Balance Management](#using-a-blockchain-for-account-and-balance-management)

<!-- TOC end -->

<!-- TOC --><a name="thunder-pool"></a>
# Thunder Pool 

A open source reference implementation of a mining pool using SRI and enabling multiple payment schemes and payouts such as L2 and L1.

<!-- TOC --><a name="overview"></a>
## Overview

In the face of creating a whole billing and payout database to maintain balances and etc, we thought that should be easy to use something like a ledger, in the persuit to design and architect a resiliant and verifiable mining pool.

Considering we are using the Stratum V2 SRI implementation as a scaffold for our mining pool.


<!-- TOC --><a name="database"></a>
### Database 

Starting from the database considering this our v1.

<!-- TOC --><a name="accounts"></a>
#### Accounts 

```sql
CREATE TABLE Accounts (
    AccountID INT AUTO_INCREMENT PRIMARY KEY,
    Username VARCHAR(255) NOT NULL,
    Email VARCHAR(255)
);

CREATE TABLE AccountBalances (
    AccountID INT,
    Balance DECIMAL(14, 8) NOT NULL DEFAULT 0,
    LastUpdated DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (AccountID) REFERENCES Accounts(AccountID),
    PRIMARY KEY (AccountID)
);
```

<!-- TOC --><a name="workers-and-shares"></a>
#### Workers and Shares 

```sql
CREATE TABLE Workers (
    WorkerID INT AUTO_INCREMENT PRIMARY KEY,
    AccountID INT,
    WorkerName VARCHAR(255) NOT NULL,
    IsActive BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (AccountID) REFERENCES Accounts(AccountID)
);

CREATE TABLE SharesContributions (
    ContributionID INT AUTO_INCREMENT PRIMARY KEY,
    WorkerID INT,
    Epoch INT NOT NULL,
    ShareValue DECIMAL(10, 2),
    Timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (WorkerID) REFERENCES Workers(WorkerID)
);


CREATE TABLE SharesStatements (
    StatementID INT AUTO_INCREMENT PRIMARY KEY,
    AccountID INT,
    Epoch INT NOT NULL,
    TotalSharesContributed DECIMAL(10, 2),
    RewardEarned DECIMAL(14, 8),
    Timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (AccountID) REFERENCES Accounts(AccountID)
);
```

<!-- TOC --><a name="payment-scheme"></a>
#### Payment Scheme

We should be able to support interval such as payment per block (10 minutes + 100 blocks usually) or payment per N minutes.

In order to this to make any sense it needs besides keeping track we must consider more things:

```sql
CREATE TABLE PaymentSchemes (
    PaymentSchemeID INT AUTO_INCREMENT PRIMARY KEY,
    SchemeName VARCHAR(255) NOT NULL,
    BlockInterval INT NULL,
    TimeInterval INT NULL,  -- Time in minutes
    Description TEXT
);
```

<!-- TOC --><a name="how-do-we-relate-the-account-with-the-paymentintervals-"></a>
##### How do we relate the account with the PaymentIntervals ?

The account could be identified with a valid bitcoin address, in order to make it easy for the ux we could use different stratum ports to tag the account with the specific payment scheme, some constraints must be taken in consideration, ideally one account -> payment intervals.

Depending on the interval we could use some syntethic asset and create a dutch auction like, or maybe we just want to advance the payment on a L2 network using the "pool btc collateral".


```sql
ALTER TABLE Accounts
ADD COLUMN PaymentSchemeID INT,
ADD COLUMN PaymentSchemeTag VARCHAR(255),
ADD FOREIGN KEY (PaymentSchemeID) REFERENCES PaymentSchemes(PaymentSchemeID);
```

<!-- TOC --><a name="how-do-we-check-the-payment-eligibility-for-an-account-"></a>
##### How do we check the payment eligibility for an account ?

<!-- TOC --><a name="using-the-payment-scheme"></a>
###### Using the Payment Scheme
```rust
fn check_payment_eligibility(account_id: i32, current_block: i32, current_time: DateTime) -> bool {
    let account_info = fetch_account_info(account_id);  // Fetch account's payment scheme details
    let scheme = fetch_payment_scheme(account_info.payment_scheme_id);
    let last_payment_info = fetch_last_payment_info_for_account(account_id);

    let blocks_since_last_payment = current_block - last_payment_info.last_block;
    let minutes_since_last_payment = (current_time - last_payment_info.last_payment_time).num_minutes();

    // Optionally, utilize account_info.payment_scheme_tag for customized checks

    if let Some(block_interval) = scheme.block_interval {
        if blocks_since_last_payment >= block_interval {
            return true;
        }
    }

    if let Some(time_interval) = scheme.time_interval {
        if minutes_since_last_payment >= time_interval {
            return true;
        }
    }

    false
}

```

<!-- TOC --><a name="not-using-the-payment-scheme"></a>
###### Not using the Payment Scheme:
```rust
fn check_payment_eligibility(account_id: i32, current_block: i32, current_time: DateTime) -> bool {
    let interval = fetch_payment_interval_for_account(account_id);
    let last_payment_info = fetch_last_payment_info_for_account(account_id);

    let blocks_since_last_payment = current_block - last_payment_info.last_block;
    let minutes_since_last_payment = (current_time - last_payment_info.last_payment_time).num_minutes();

    if interval.block_interval.is_some() && blocks_since_last_payment >= interval.block_interval.unwrap() {
        return true;
    }

    if interval.time_interval.is_some() && minutes_since_last_payment >= interval.time_interval.unwrap() {
        return true;
    }

    false
}
```

<!-- TOC --><a name="architecture-overview"></a>
## Architecture Overview 

<!-- TOC --><a name="stratum-protocol-implementation-v1v2"></a>
### Stratum Protocol Implementation v1/v2

<!-- TOC --><a name="template-provider"></a>
### Template Provider

<!-- TOC --><a name="mining-pool-and-translator-proxy"></a>
### Mining Pool and Translator Proxy

<!-- TOC --><a name="job-declarator-serverclient"></a>
### Job Declarator server/client

<!-- TOC --><a name="idea-box"></a>
## Idea box

<!-- TOC --><a name="making-merkle-trees-work-for-us"></a>
### Making Merkle Trees Work for Us:

Auditability: The inherent structure of Merkle trees facilitates the straightforward auditability of all contributions. Historical data can be verified against the Merkle root anytime, ensuring transparency and trustworthiness.

<!-- TOC --><a name="building-the-tree"></a>
#### Building the Tree:

- **New Contributions**: With each new contribution, we serialize essential details (like contributor ID, job ID, share amount, and timestamp) and add them as a leaf node to the Merkle tree. Following the addition of new contributions, the Merkle root is recalculated to reflect the updated data.
- **Proof Storage**: Alongside each contribution record in the database, we store its corresponding Merkle proof—the series of hashes necessary to trace the path back to the root. This setup enables individual contribution verifications without the need to reconstruct the entire tree.

- **Contribution Verification**: To verify a contribution, we retrieve its Merkle proof from the database and validate it against the stored Merkle root using the contribution's data. Successful verification confirms the contribution's presence in the dataset at the root's last calculation.

- **Using a blockchain**: 

<!-- TOC --><a name="validating-contributions"></a>
#### Validating Contributions:

- **Proof in the Pudding**: To confirm a contribution's legit, we pull its Merkle proof and use it to retrace its steps to the root. If things line up, we're golden—the contribution was part of our dataset when the root was last calculated.

<!-- TOC --><a name="direct-blockchain-integration-for-balance-management"></a>
#### Direct Blockchain Integration for Balance Management

Under this approach, each miner's account balance and transactions are recorded directly on the blockchain. This could involve the creation of individual blockchain addresses for miners or smart contracts to manage account balances within the mining pool.

<!-- TOC --><a name="advantages-of-direct-blockchain-balance-management"></a>
##### Advantages of Direct Blockchain Balance Management

- **Simplified Implementation**: Without the complexity of minting "bitassets" and managing a separate DEX, this approach might offer a more straightforward path to leveraging blockchain for balance management.

- **Direct Blockchain Security**: Utilizing blockchain directly for balance tracking harnesses the inherent security and immutability of blockchain technology, ensuring that account balances are resistant to tampering and fraud.

- **Transparent Transactions**: Every transaction, including rewards distribution and withdrawals, can be verified on the blockchain, providing miners with unparalleled transparency regarding their earnings.

- **Smart Contract Automation**: Smart contracts can automate the distribution of mining rewards based on the shares submitted, directly updating each miner's blockchain-based account balance. This automation reduces the need for manual processing and potential errors.

<!-- TOC --><a name="using-a-blockchain-for-account-and-balance-management"></a>
##### Using a Blockchain for Account and Balance Management

Integrating blockchain technology into our mining pool operation for handling "accounts and balance" logic offers a transformative shift from traditional databases to a ledger-based approach. Specifically, the use of "bitassets" on a drivechain allows us to mint assets representing epochs of mining contributions. These assets can then be utilized in a decentralized exchange (DEX) through a Dutch auction mechanism, with "BTC or Thunder BTC" serving as collateral. This strategy brings forth several advantages:

Advantages of Blockchain Integration
- **Decentralization and Security**: Leveraging blockchain technology decentralizes account and balance management, distributing data across multiple nodes. This not only enhances security against data tampering but also promotes transparency in the mining process.

- **Tokenization of Shares**: By minting "bitassets" that represent epochs of mining contributions, we tokenize mining efforts, enabling miners to own, trade, or leverage their contributions as digital assets. This tokenization opens new avenues for miners to capitalize on their work beyond traditional reward systems.

- **Innovative Trading Mechanisms**: The creation of a DEX for these "bitassets" introduces a dynamic market where mining contributions have liquidity. The use of a Dutch auction system for trading these assets with BTC or Thunder BTC as collateral ensures fair market pricing and provides miners with immediate value for their contributions.

Enhanced Liquidity and Access to Capital: Tokenizing mining contributions and facilitating their trade on a DEX significantly increases liquidity, providing miners with quicker access to capital. This is particularly advantageous during market fluctuations, as miners can choose to hold or sell their assets based on market conditions.

Automated and Transparent Payouts: With smart contracts governing the distribution of rewards and handling of auctions, payouts and transactions become more transparent, timely, and resistant to manipulation. Miners can trust in an unbiased system to receive their due rewards.

- **Legal and Regulatory Compliance**: Tokenizing mining contributions and facilitating their trade might subject the operation to financial regulations. Navigating this landscape requires careful planning and potentially legal consultation to ensure compliance.
