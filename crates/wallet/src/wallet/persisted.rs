use core::fmt;

use crate::{descriptor::DescriptorError, Wallet};

/// Represents a persisted wallet.
pub type PersistedWallet = bdk_chain::Persisted<Wallet>;

#[cfg(feature = "sqlite")]
impl<'c> chain::PersistWith<bdk_chain::sqlite::Transaction<'c>> for Wallet {
    type CreateParams = crate::CreateParams;
    type LoadParams = crate::LoadParams;

    type CreateError = CreateWithPersistError<bdk_chain::rusqlite::Error>;
    type LoadError = LoadWithPersistError<bdk_chain::rusqlite::Error>;
    type PersistError = bdk_chain::rusqlite::Error;

    fn create(
        db: &mut bdk_chain::sqlite::Transaction<'c>,
        params: Self::CreateParams,
    ) -> Result<Self, Self::CreateError> {
        let mut wallet =
            Self::create_with_params(params).map_err(CreateWithPersistError::Descriptor)?;
        if let Some(changeset) = wallet.take_staged() {
            changeset
                .persist_to_sqlite(db)
                .map_err(CreateWithPersistError::Persist)?;
        }
        Ok(wallet)
    }

    fn load(
        conn: &mut bdk_chain::sqlite::Transaction<'c>,
        params: Self::LoadParams,
    ) -> Result<Option<Self>, Self::LoadError> {
        let changeset =
            crate::ChangeSet::from_sqlite(conn).map_err(LoadWithPersistError::Persist)?;
        if chain::Merge::is_empty(&changeset) {
            return Ok(None);
        }
        Self::load_with_params(changeset, params).map_err(LoadWithPersistError::InvalidChangeSet)
    }

    fn persist(
        &mut self,
        conn: &mut bdk_chain::sqlite::Transaction,
    ) -> Result<bool, Self::PersistError> {
        if let Some(changeset) = self.take_staged() {
            changeset.persist_to_sqlite(conn)?;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(feature = "sqlite")]
impl chain::PersistWith<bdk_chain::sqlite::Connection> for Wallet {
    type CreateParams = crate::CreateParams;
    type LoadParams = crate::LoadParams;

    type CreateError = CreateWithPersistError<bdk_chain::rusqlite::Error>;
    type LoadError = LoadWithPersistError<bdk_chain::rusqlite::Error>;
    type PersistError = bdk_chain::rusqlite::Error;

    fn create(
        db: &mut bdk_chain::sqlite::Connection,
        params: Self::CreateParams,
    ) -> Result<Self, Self::CreateError> {
        let mut db_tx = db.transaction().map_err(CreateWithPersistError::Persist)?;
        let wallet = chain::PersistWith::create(&mut db_tx, params)?;
        db_tx.commit().map_err(CreateWithPersistError::Persist)?;
        Ok(wallet)
    }

    fn load(
        db: &mut bdk_chain::sqlite::Connection,
        params: Self::LoadParams,
    ) -> Result<Option<Self>, Self::LoadError> {
        let mut db_tx = db.transaction().map_err(LoadWithPersistError::Persist)?;
        let wallet_opt = chain::PersistWith::load(&mut db_tx, params)?;
        db_tx.commit().map_err(LoadWithPersistError::Persist)?;
        Ok(wallet_opt)
    }

    fn persist(
        &mut self,
        db: &mut bdk_chain::sqlite::Connection,
    ) -> Result<bool, Self::PersistError> {
        let mut db_tx = db.transaction()?;
        let has_changes = chain::PersistWith::persist(self, &mut db_tx)?;
        db_tx.commit()?;
        Ok(has_changes)
    }
}

#[cfg(feature = "file_store")]
impl chain::PersistWith<bdk_file_store::Store<crate::ChangeSet>> for Wallet {
    type CreateParams = crate::CreateParams;
    type LoadParams = crate::LoadParams;
    type CreateError = CreateWithPersistError<std::io::Error>;
    type LoadError =
        LoadWithPersistError<bdk_file_store::AggregateChangesetsError<crate::ChangeSet>>;
    type PersistError = std::io::Error;

    fn create(
        db: &mut bdk_file_store::Store<crate::ChangeSet>,
        params: Self::CreateParams,
    ) -> Result<Self, Self::CreateError> {
        let mut wallet =
            Self::create_with_params(params).map_err(CreateWithPersistError::Descriptor)?;
        if let Some(changeset) = wallet.take_staged() {
            db.append_changeset(&changeset)
                .map_err(CreateWithPersistError::Persist)?;
        }
        Ok(wallet)
    }

    fn load(
        db: &mut bdk_file_store::Store<crate::ChangeSet>,
        params: Self::LoadParams,
    ) -> Result<Option<Self>, Self::LoadError> {
        let changeset = db
            .aggregate_changesets()
            .map_err(LoadWithPersistError::Persist)?
            .unwrap_or_default();
        Self::load_with_params(changeset, params).map_err(LoadWithPersistError::InvalidChangeSet)
    }

    fn persist(
        &mut self,
        db: &mut bdk_file_store::Store<crate::ChangeSet>,
    ) -> Result<bool, Self::PersistError> {
        if let Some(changeset) = self.take_staged() {
            db.append_changeset(&changeset)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// Error type for [`PersistedWallet::load`].
#[derive(Debug)]
pub enum LoadWithPersistError<E> {
    /// Error from persistence.
    Persist(E),
    /// Occurs when the loaded changeset cannot construct [`Wallet`].
    InvalidChangeSet(crate::LoadError),
}

impl<E: fmt::Display> fmt::Display for LoadWithPersistError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persist(err) => fmt::Display::fmt(err, f),
            Self::InvalidChangeSet(err) => fmt::Display::fmt(&err, f),
        }
    }
}

#[cfg(feature = "std")]
impl<E: fmt::Debug + fmt::Display> std::error::Error for LoadWithPersistError<E> {}

/// Error type for [`PersistedWallet::create`].
#[derive(Debug)]
pub enum CreateWithPersistError<E> {
    /// Error from persistence.
    Persist(E),
    /// Occurs when the loaded changeset cannot contruct [`Wallet`].
    Descriptor(DescriptorError),
}

impl<E: fmt::Display> fmt::Display for CreateWithPersistError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persist(err) => fmt::Display::fmt(err, f),
            Self::Descriptor(err) => fmt::Display::fmt(&err, f),
        }
    }
}

#[cfg(feature = "std")]
impl<E: fmt::Debug + fmt::Display> std::error::Error for CreateWithPersistError<E> {}
