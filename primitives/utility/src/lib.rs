// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Helper datatypes for cumulus. This includes the [`ParentAsUmp`] routing type which will route
//! messages into an [`UpwardMessageSender`] if the destination is `Parent`.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;
use cumulus_primitives_core::{MessageSendError, UpwardMessageSender};
use frame_support::traits::Get;
use polkadot_runtime_common::xcm_sender::ConstantPrice;
use sp_std::{marker::PhantomData, prelude::*};
use xcm::{latest::prelude::*, WrapVersion};

pub trait PriceForParentDelivery {
	fn price_for_parent_delivery(message: &Xcm<()>) -> MultiAssets;
}

impl PriceForParentDelivery for () {
	fn price_for_parent_delivery(_: &Xcm<()>) -> MultiAssets {
		MultiAssets::new()
	}
}

impl<T: Get<MultiAssets>> PriceForParentDelivery for ConstantPrice<T> {
	fn price_for_parent_delivery(_: &Xcm<()>) -> MultiAssets {
		T::get()
	}
}

/// Xcm router which recognises the `Parent` destination and handles it by sending the message into
/// the given UMP `UpwardMessageSender` implementation. Thus this essentially adapts an
/// `UpwardMessageSender` trait impl into a `SendXcm` trait impl.
///
/// NOTE: This is a pretty dumb "just send it" router; we will probably want to introduce queuing
/// to UMP eventually and when we do, the pallet which implements the queuing will be responsible
/// for the `SendXcm` implementation.
pub struct ParentAsUmp<T, W, P>(PhantomData<(T, W, P)>);
impl<T, W, P> SendXcm for ParentAsUmp<T, W, P>
where
	T: UpwardMessageSender,
	W: WrapVersion,
	P: PriceForParentDelivery,
{
	type Ticket = Vec<u8>;

	fn validate(
		dest: &mut Option<MultiLocation>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Vec<u8>> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;
		let xcm = msg.take().ok_or(SendError::MissingArgument)?;

		if d.contains_parents_only(1) {
			// An upward message for the relay chain.
			let price = P::price_for_parent_delivery(&xcm);
			let versioned_xcm =
				W::wrap_version(&d, xcm).map_err(|()| SendError::DestinationUnsupported)?;
			let data = versioned_xcm.encode();

			Ok((data, price))
		} else {
			*dest = Some(d);
			// Anything else is unhandled. This includes a message this is meant for us.
			Err(SendError::NotApplicable)
		}
	}

	fn deliver(data: Vec<u8>) -> Result<XcmHash, SendError> {
		let hash = data.using_encoded(sp_io::hashing::blake2_256);

		T::send_upward_message(data).map_err(|e| match e {
			MessageSendError::TooBig => SendError::ExceedsMaxMessageSize,
			e => SendError::Transport(e.into()),
		})?;

		Ok(hash)
	}
}
