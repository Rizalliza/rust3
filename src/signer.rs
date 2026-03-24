use anyhow::Context;
use solana_sdk::message::VersionedMessage;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::VersionedTransaction;

pub trait TransactionSigner: Send + Sync {
    fn pubkey(&self) -> Pubkey;
    fn sign_versioned_message(
        &self,
        message: VersionedMessage,
    ) -> anyhow::Result<VersionedTransaction>;
}

pub struct LocalKeypairSigner {
    keypair: Keypair,
}

impl LocalKeypairSigner {
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }
}

impl TransactionSigner for LocalKeypairSigner {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    fn sign_versioned_message(
        &self,
        message: VersionedMessage,
    ) -> anyhow::Result<VersionedTransaction> {
        VersionedTransaction::try_new(message, &[&self.keypair])
            .context("failed to sign versioned transaction with local keypair")
    }
}
