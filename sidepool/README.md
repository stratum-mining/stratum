# sidepool

A open source reference implementation of a mining pool using SRI and enabling multiple payment schemes and payouts such as L2 and L1.

## Overview

In the face of creating a whole billing and payout database to maintain balances and etc, we thought that should be easy to use something like a ledger, in the persuit to design and architect a resiliant and verifiable mining pool.

Considering we are using the Stratum V2 SRI implementation as a scaffold for our mining pool.


## setting up sidepool

1. Build all the roles: build_roles.sh
2. Run Bitcoin Signet if intented to test: run_bitcoin_signet.sh
3. Run docker-compose.yml

docker-compose build --build-arg REPO_URL=https://github.com/rsantacroce/stratum.git
docker-compose up

### cpu miner 
download your platform specific version from: https://github.com/pooler/cpuminer

    ./minerd -a sha256d -o stratum+tcp://localhost:34255 -u satoshi -p satoshi -q -D -P

### running the tp

Use the following script to download the latest bitcoin_signet docker files, build and run the network on local env:
    
    run_bitcoin_signet.sh

There is a few hand scripts that can be used to spam tx to the network and also mine signet coins using the bitcoin-cli, usually you just need to get the bash from the container and execute it:

    docker exec -it bitcoin-signet-instance /bin/bash
    send_tx.sh number-of-txs duration
    mine.sh 

### DRAFT db  

Starting from the database considering this our v1.

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
