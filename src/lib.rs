#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure, Parameter};
use frame_system::ensure_signed;
use codec::{Decode, Encode};
use sp_runtime::{DispatchResult, RuntimeDebug};
use sp_runtime::traits::{
    AtLeast32Bit, AtLeast32BitUnsigned, CheckedSub, MaybeSerializeDeserialize, Member, One, Saturating, StaticLookup,
    Zero,
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

type Symbol = [u8; 8];
type Name = [u8; 16];

#[derive(Encode, Decode, Eq, PartialEq, Clone, RuntimeDebug, Default)]
pub struct AssetInfo {
    pub name: Name,
    pub symbol: Symbol,
    pub decimals: u8,
}

pub trait Trait: frame_system::Trait {
    type TokenBalance: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaybeSerializeDeserialize;
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    type AssetId: Parameter + AtLeast32Bit + Default + Copy + MaybeSerializeDeserialize;
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;
        
        #[weight = 0]
        fn issue(origin, #[compact] total: T::TokenBalance, asset_info: AssetInfo) {
            let origin = ensure_signed(origin)?;
            Self::inner_issue(&origin, total, &asset_info);
        }

        #[weight = 0]
        fn approve(origin,
            #[compact] id: T::AssetId,
            spender: <T::Lookup as StaticLookup>::Source,
            #[compact] amount: T::TokenBalance
        ) {
            let owner = ensure_signed(origin)?;
            let spender = T::Lookup::lookup(spender)?;

            Self::inner_approve(&id, &owner, &spender, amount)?;
        }
        
        #[weight = 0]
        fn transfer(origin,
            #[compact] id: T::AssetId,
            target: <T::Lookup as StaticLookup>::Source,
            #[compact] amount: T::TokenBalance
        ) {
            let origin = ensure_signed(origin)?;
            let target = T::Lookup::lookup(target)?;

            Self::inner_transfer(&id, &origin, &target, amount)?;
        }

        #[weight = 0]
        fn transfer_from(origin,
            #[compact] id: T::AssetId,
            from: <T::Lookup as StaticLookup>::Source,
            target: <T::Lookup as StaticLookup>::Source,
            #[compact] amount: T::TokenBalance
        ){
            let spender = ensure_signed(origin)?;
            let owner = T::Lookup::lookup(from)?;
            let target = T::Lookup::lookup(target)?;

            Self::inner_transfer_from(&id, &owner, &spender, &target, amount)?;
        }
    }
}

decl_event! {
    pub enum Event<T> where
        <T as frame_system::Trait>::AccountId,
        <T as Trait>::TokenBalance,
        <T as Trait>::AssetId,
    {
        Issued(AssetId, AccountId, TokenBalance),
        Transferred(AssetId, AccountId, AccountId, TokenBalance),
        Approval(AssetId, AccountId, AccountId, TokenBalance),

        Minted(AssetId, AccountId, TokenBalance),
        Burned(AssetId, AccountId, TokenBalance),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        BalanceLow,
        BalanceZero,
        AllowanceLow,
        AmountZero,
        AssetNotExists,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Assets {
        TotalSupply: map hasher(twox_64_concat) T::AssetId => T::TokenBalance;
        AssetInfos: map hasher(twox_64_concat) T::AssetId => Option<AssetInfo>;
        Balances: map hasher(blake2_128_concat) (T::AssetId, T::AccountId) => T::TokenBalance;
        NextAssetId get(fn next_asset_id): T::AssetId;
        Allowances: map hasher(blake2_128_concat) (T::AssetId, T::AccountId, T::AccountId) => T::TokenBalance;
    }
}

impl<T: Trait> Module<T> {
    pub fn total_supply(id: &T::AssetId) -> T::TokenBalance {
        <TotalSupply<T>>::get(id)
    }

    pub fn balance_of(id: &T::AssetId, owner: &T::AccountId) -> T::TokenBalance {
        <Balances<T>>::get((id, owner))
    }

    pub fn inner_issue(
        owner: &T::AccountId,
        initial_supply: T::TokenBalance,
        info: &AssetInfo,
    ) -> T::AssetId {
        let id = Self::next_asset_id();
        <NextAssetId<T>>::mutate(|id| *id += One::one());

        <Balances<T>>::insert((id, owner), initial_supply);
        <TotalSupply<T>>::insert(id, initial_supply);
        <AssetInfos<T>>::insert(id, info);

        Self::deposit_event(RawEvent::Issued(id, owner.clone(), initial_supply));

        id
    }

    pub fn asset_info(id: &T::AssetId) -> Option<AssetInfo> {
        <AssetInfos<T>>::get(id)
    }

    pub fn inner_transfer(
        id: &T::AssetId,
        owner: &T::AccountId,
        target: &T::AccountId,
        amount: T::TokenBalance,
    ) -> DispatchResult {
        let owner_balance = <Balances<T>>::get((id, owner));
        ensure!(!amount.is_zero(), Error::<T>::AmountZero);
        ensure!(owner_balance >= amount, Error::<T>::BalanceLow);

        let new_balance = owner_balance.saturating_sub(amount);

        <Balances<T>>::mutate((id, owner), |balance| *balance = new_balance);
        <Balances<T>>::mutate((id, target), |balance| {
            *balance = balance.saturating_add(amount)
        });

        Self::deposit_event(RawEvent::Transferred(
            *id,
            owner.clone(),
            target.clone(),
            amount,
        ));

        Ok(())
    }

    pub fn inner_transfer_from(
        id: &T::AssetId,
        owner: &T::AccountId,
        spender: &T::AccountId,
        target: &T::AccountId,
        amount: T::TokenBalance,
    ) -> DispatchResult {
        let allowance = <Allowances<T>>::get((id, owner, spender));
        let new_balance = allowance
            .checked_sub(&amount)
            .ok_or(Error::<T>::AllowanceLow)?;

        Self::inner_transfer(&id, &owner, &target, amount)?;

        <Allowances<T>>::mutate((id, owner, spender), |balance| *balance = new_balance);

        Ok(())
    }

    pub fn inner_approve(
        id: &T::AssetId,
        owner: &T::AccountId,
        spender: &T::AccountId,
        amount: T::TokenBalance,
    ) -> DispatchResult {
        <Allowances<T>>::mutate((id, owner, spender), |balance| *balance = amount);

        Self::deposit_event(RawEvent::Approval(
            *id,
            owner.clone(),
            spender.clone(),
            amount,
        ));

        Ok(())
    }

    pub fn allowances(id: &T::AssetId, owner: &T::AccountId, spender: &T::AccountId) -> T::TokenBalance {
        <Allowances<T>>::get((id, owner, spender))
    }

    pub fn inner_mint(id: &T::AssetId, owner: &T::AccountId, amount: T::TokenBalance) -> DispatchResult {
        ensure!(Self::asset_info(id).is_some(), Error::<T>::AssetNotExists);

        let new_balance = <Balances<T>>::get((id, owner)).saturating_add(amount);

        <Balances<T>>::mutate((id, owner), |balance| *balance = new_balance);
        <TotalSupply<T>>::mutate(id, |supply| {
            *supply = supply.saturating_add(amount);
        });

        Self::deposit_event(RawEvent::Minted(*id, owner.clone(), amount));

        Ok(())
    }

    pub fn inner_burn(id: &T::AssetId, owner: &T::AccountId, amount: T::TokenBalance) -> DispatchResult {
        ensure!(Self::asset_info(id).is_some(), Error::<T>::AssetNotExists);

        let new_balance = <Balances<T>>::get((id, owner))
            .checked_sub(&amount)
            .ok_or(Error::<T>::BalanceLow)?;

        <Balances<T>>::mutate((id, owner), |balance| *balance = new_balance);
        <TotalSupply<T>>::mutate(id, |supply| {
            *supply = supply.saturating_sub(amount);
        });

        Self::deposit_event(RawEvent::Burned(*id, owner.clone(), amount));

        Ok(())
    }
}