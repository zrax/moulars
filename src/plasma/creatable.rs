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

use std::fmt::{Debug, Formatter};

use anyhow::{anyhow, Result};
use num_derive::FromPrimitive;

use super::{StreamRead, StreamWrite};
use super::messages::NetSafety;

pub trait Creatable: StreamRead + StreamWrite {
    fn class_id(&self) -> u16;
    fn static_class_id() -> u16
        where Self: Sized;

    fn have_interface(&self, class_id: ClassID) -> bool;
    fn check_interface(&self, class_id: ClassID) -> Result<()> {
        if !self.have_interface(class_id) {
            Err(anyhow!("Unexpected creatable type 0x{:04x} (Expected {:?} interface)",
                        self.class_id(), class_id))
        } else {
            Ok(())
        }
    }

    fn as_creatable(&self) -> &dyn Creatable;
    fn net_safety_mut(&mut self) -> Option<&mut dyn NetSafety> { None }
}

impl Debug for dyn Creatable {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "Creatable {{ type=0x{:04x} }}", self.class_id())
    }
}

#[repr(u16)]
#[derive(FromPrimitive, Debug, Copy, Clone)]
pub enum ClassID {
    SoundBuffer = 0x0029,
    //CoopCoordinator = 0x011B,
    RelevanceRegion = 0x011E,
    Message = 0x0202,
    AnimCmdMsg = 0x0206,
    //InputEventMsg = 0x020B,
    //ControlEventMsg = 0x0210,
    //NetMsgPagingRoom = 0x0218,
    //LoadCloneMsg = 0x0253,
    //EnableMsg = 0x0254,
    //WarpMsg = 0x0255,
    //NetMsgGroupOwner = 0x0264,
    //NetMsgGameStateRequest = 0x0265,
    //NetMsgGameMessage = 0x026B,
    //ServerReplyMsg = 0x026F,
    //NetMsgVoice = 0x0279,
    //NetMsgTestAndSet = 0x027D,
    MessageWithCallbacks = 0x0283,
    //AvTaskMsg = 0x0298,
    //AvSeekMsg = 0x0299,
    //AvOneShotMsg = 0x029A,
    //NetMsgMembersListReq = 0x02AD,
    //NetMsgMembersList = 0x02AE,
    //NetMsgMemberUpdate = 0x02B1,
    //NetMsgInitialAgeStateSent = 0x02B8,
    //AvTaskSeekDoneMsg = 0x02C0,
    //AgeLinkStruct = 0x02C4,
    //NetMsgSDLState = 0x02CD,
    //LinkToAgeMsg = 0x02E6,
    //NotifyMsg = 0x02ED,
    //LinkEffectsTriggerMsg = 0x0300,
    //NetMsgSDLStateBCast = 0x0329,
    //NetMsgGameMessageDirected = 0x032E,
    //ParticleTransferMsg = 0x0333,
    //ParticleKillMsg = 0x0334,
    //AvatarInputStateMsg = 0x0347,
    //AgeInfoStruct = 0x0348,
    LinkingMgrMsg = 0x034B,
    //ClothingMsg = 0x0357,
    //AvBrainHuman = 0x035C,
    //AvBrainCritter = 0x035D,
    //AvBrainDrive = 0x035E,
    //AvBrainGeneric = 0x0360,
    //InputIfaceMgrMsg = 0x0363,
    //KIMessage = 0x0364,
    //AvPushBrainMsg = 0x0367,
    //AvPopBrainMsg = 0x0368,
    //AvAnimTask = 0x036B,
    //AvSeekTask = 0x036C,
    //AvOneShotTask = 0x036E,
    //AvTaskBrain = 0x0370,
    //AnimStage = 0x0371,
    CreatableGenericValue = 0x038C,
    //AvBrainGenericMsg = 0x038F,
    //AvTaskSeek = 0x0390,
    //MultistageModMsg = 0x03A3,
    //BulletMsg = 0x03A6,
    //NetMsgRelevanceRegions = 0x03AC,
    //LoadAvatarMsg = 0x03B1,
    //NetMsgLoadClone = 0x03B3,
    //NetMsgPlayerPage = 0x03B4,
    //SubWorldMsg = 0x03BF,
    //AvBrainSwim = 0x042D,
    //ClimbMsg = 0x0451,
    //AvBrainClimb = 0x0453,
    //AvCoopMsg = 0x045E,
    //AvBrainCoop = 0x045F,
    //SetNetGroupIdMsg = 0x0464,
    //BackdoorMsg = 0x0465,
    //AvOneShotLinkTask = 0x0488,
    //PseudoLinkEffectMsg = 0x0494,
    //AvBrainRideAnimatedPhysical = 0x049E,

    Nil = 0x8000,
}

macro_rules! derive_creatable {
    ($name:ident) => { derive_creatable! { $name, () } };
    ($name:ident, ($($have_ifc:ident),* $(,)?)) => {
        derive_creatable! { $name @lines[
            fn have_interface(&self, class_id: $crate::plasma::creatable::ClassID) -> bool {
                matches!(class_id, $crate::plasma::creatable::ClassID::$name
                                   $(| $crate::plasma::creatable::ClassID::$have_ifc)*)
            }
        ] }
    };
    ($name:ident, NetSafety, ($($have_ifc:ident),+ $(,)?)) => {
        derive_creatable! { $name @lines[
            fn have_interface(&self, class_id: $crate::plasma::creatable::ClassID) -> bool {
                matches!(class_id, $crate::plasma::creatable::ClassID::$name
                                   $(| $crate::plasma::creatable::ClassID::$have_ifc)*)
            }
            fn net_safety_mut(&mut self) -> Option<&mut dyn NetSafety> { Some(self) }
        ] }
    };
    ($name:ident @lines[ $($lines:tt)* ]) => {
        impl $crate::plasma::Creatable for $name {
            fn class_id(&self) -> u16 { $crate::plasma::creatable::ClassID::$name as u16 }
            fn static_class_id() -> u16 { $crate::plasma::creatable::ClassID::$name as u16 }
            fn as_creatable(&self) -> &dyn $crate::plasma::Creatable { self }
            $($lines)*
        }
    };
}
pub(crate) use derive_creatable;
