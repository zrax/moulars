/* This file is part of moulars.
 *
 * moulars is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * moulars is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with moulars.  If not, see <http://www.gnu.org/licenses/>.
 */

pub type NetResult<T> = Result<T, NetResultCode>;

#[repr(i32)]
#[derive(Eq, PartialEq, Debug)]
pub enum NetResultCode
{
    NetPending = -1,
    NetSuccess = 0,
    NetInternalError,
    NetTimeout,
    NetBadServerData,
    NetAgeNotFound,
    NetConnectFailed,
    NetDisconnected,
    NetFileNotFound,
    NetOldBuildId,
    NetRemoteShutdown,
    NetTimeoutOdbc,
    NetAccountAlreadyExists,
    NetPlayerAlreadyExists,
    NetAccountNotFound,
    NetPlayerNotFound,
    NetInvalidParameter,
    NetNameLookupFailed,
    NetLoggedInElsewhere,
    NetVaultNodeNotFound,
    NetMaxPlayersOnAcct,
    NetAuthenticationFailed,
    NetStateObjectNotFound,
    NetLoginDenied,
    NetCircularReference,
    NetAccountNotActivated,
    NetKeyAlreadyUsed,
    NetKeyNotFound,
    NetActivationCodeNotFound,
    NetPlayerNameInvalid,
    NetNotSupported,
    NetServiceForbidden,
    NetAuthTokenTooOld,
    NetMustUseGameTapClient,
    NetTooManyFailedLogins,
    NetGameTapConnectionFailed,
    NetGTTooManyAuthOptions,
    NetGTMissingParameter,
    NetGTServerError,
    NetAccountBanned,
    NetKickedByCCR,
    NetScoreWrongType,
    NetScoreNotEnoughPoints,
    NetScoreAlreadyExists,
    NetScoreNoDataFound,
    NetInviteNoMatchingPlayer,
    NetInviteTooManyHoods,
    NetNeedToPay,
    NetServerBusy,
}
