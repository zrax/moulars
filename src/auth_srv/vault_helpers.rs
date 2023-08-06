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

use log::warn;
use uuid::Uuid;

use crate::netcli::{NetResult, NetResultCode};
use crate::sdl;
use crate::vault::{VaultServer, VaultNode, StandardNode, PlayerInfo, GameServer};

pub async fn create_player_nodes(account_id: &Uuid, player: &PlayerInfo,
                                 vault: &VaultServer) -> NetResult<()>
{
    let node = VaultNode::new_player_info(account_id, player.player_id, &player.player_name);
    let player_info = vault.create_node(node).await?;

    let node = VaultNode::new_player_info_list(account_id, player.player_id,
                                               StandardNode::BuddyListFolder);
    let buddy_list = vault.create_node(node).await?;

    let node = VaultNode::new_player_info_list(account_id, player.player_id,
                                               StandardNode::IgnoreListFolder);
    let ignore_list = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::PlayerInviteFolder);
    let invite_folder = vault.create_node(node).await?;

    let node = VaultNode::new_age_info_list(account_id, player.player_id,
                                            StandardNode::AgesIOwnFolder);
    let owned_ages = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::AgeJournalsFolder);
    let journals_folder = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::ChronicleFolder);
    let chronicle_folder = vault.create_node(node).await?;

    let node = VaultNode::new_age_info_list(account_id, player.player_id,
                                            StandardNode::AgesICanVisitFolder);
    let visit_ages = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::AvatarOutfitFolder);
    let outfit_folder = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::AvatarClosetFolder);
    let closet_folder = vault.create_node(node).await?;

    let node = VaultNode::new_folder(account_id, player.player_id,
                                     StandardNode::InboxFolder);
    let inbox = vault.create_node(node).await?;

    let node = VaultNode::new_player_info_list(account_id, player.player_id,
                                               StandardNode::PeopleIKnowAboutFolder);
    let people_list = vault.create_node(node).await?;

    let node = VaultNode::new_age_link(account_id, player.player_id,
                                       "Default:LinkInPointDefault:;");
    let relto_link = vault.create_node(node).await?;

    let node = VaultNode::new_age_link(account_id, player.player_id,
                                       "Default:LinkInPointDefault:;");
    let hood_link = vault.create_node(node).await?;

    let node = VaultNode::new_age_link(account_id, player.player_id,
                                       "Ferry Terminal:LinkInPointFerry:;");
    let city_link = vault.create_node(node).await?;

    let user_name = format!("{}'s", player.player_name);
    let description = format!("{}'s Relto", player.player_name);
    let (relto_id, relto_info) = create_age_nodes(&Uuid::new_v4(), None,
            "Personal", "Relto", &user_name, &description, 0, -1, Some(player_info),
            false, vault).await?;

    // TODO: Add the new player to a 'Hood
    // TODO: Get the public city age

    let system_node = vault.get_system_node().await?;
    vault.ref_node(player.player_id, system_node, 0).await?;
    vault.ref_node(player.player_id, player_info, 0).await?;
    vault.ref_node(player.player_id, buddy_list, 0).await?;
    vault.ref_node(player.player_id, ignore_list, 0).await?;
    vault.ref_node(player.player_id, invite_folder, 0).await?;
    vault.ref_node(player.player_id, owned_ages, 0).await?;
    vault.ref_node(player.player_id, journals_folder, 0).await?;
    vault.ref_node(player.player_id, chronicle_folder, 0).await?;
    vault.ref_node(player.player_id, visit_ages, 0).await?;
    vault.ref_node(player.player_id, outfit_folder, 0).await?;
    vault.ref_node(player.player_id, closet_folder, 0).await?;
    vault.ref_node(player.player_id, inbox, 0).await?;
    vault.ref_node(player.player_id, people_list, 0).await?;
    vault.ref_node(owned_ages, relto_link, 0).await?;
    vault.ref_node(owned_ages, hood_link, 0).await?;
    vault.ref_node(owned_ages, city_link, 0).await?;
    vault.ref_node(relto_link, relto_info, 0).await?;
    /* TODO vault.ref_node(hood_link, hood_info, 0).await?; */
    /* TODO vault.ref_node(city_link, city_info, 0).await?; */
    vault.ref_node(relto_id, owned_ages, 0).await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_age_nodes(age_uuid: &Uuid, parent_uuid: Option<&Uuid>,
        age_filename: &str, instance_name: &str, user_name: &str, description: &str,
        sequence_number: i32, language: i32, add_owner: Option<u32>, public: bool,
        vault: &VaultServer) -> NetResult<(u32, u32)>
{
    let node = VaultNode::new_age(age_uuid, parent_uuid, age_filename);
    let age_id = vault.create_node(node).await?;

    let node = VaultNode::new_folder(age_uuid, age_id, StandardNode::ChronicleFolder);
    let chronicle_folder = vault.create_node(node).await?;

    let node = VaultNode::new_player_info_list(age_uuid, age_id,
                                               StandardNode::PeopleIKnowAboutFolder);
    let people_list = vault.create_node(node).await?;

    let node = VaultNode::new_age_info_list(age_uuid, age_id, StandardNode::SubAgesFolder);
    let sub_ages = vault.create_node(node).await?;

    let node = VaultNode::new_age_info(age_uuid, age_id, sequence_number, public,
                                       language, parent_uuid, age_filename,
                                       instance_name, user_name, description);
    let age_info = vault.create_node(node).await?;

    let node = VaultNode::new_folder(age_uuid, age_id, StandardNode::AgeDevicesFolder);
    let devices_folder = vault.create_node(node).await?;

    let node = VaultNode::new_player_info_list(age_uuid, age_id,
                                               StandardNode::CanVisitFolder);
    let can_visit = vault.create_node(node).await?;

    let sdl_node = if let Some(descriptor) = vault.sdl_db().get_latest(age_filename) {
        let sdl_blob = match sdl::State::from_defaults(descriptor, vault.sdl_db()).to_blob() {
            Ok(blob) => blob,
            Err(err) => {
                warn!("Failed to generate default SDL for {}: {}", age_filename, err);
                return Err(NetResultCode::NetInternalError);
            }
        };
        let node = VaultNode::new_sdl(age_uuid, age_id, age_filename, &sdl_blob);
        vault.create_node(node).await?
    } else {
        warn!("Could not find SDL descriptor for {}", age_filename);
        return Err(NetResultCode::NetInternalError);
    };

    let node = VaultNode::new_player_info_list(age_uuid, age_id,
                                               StandardNode::AgeOwnersFolder);
    let age_owners = vault.create_node(node).await?;

    let node = VaultNode::new_age_info_list(age_uuid, age_id,
                                            StandardNode::ChildAgesFolder);
    let child_ages = vault.create_node(node).await?;

    let system_node = vault.get_system_node().await?;
    vault.ref_node(age_id, system_node, 0).await?;
    vault.ref_node(age_id, chronicle_folder, 0).await?;
    vault.ref_node(age_id, people_list, 0).await?;
    vault.ref_node(age_id, sub_ages, 0).await?;
    vault.ref_node(age_id, age_info, 0).await?;
    vault.ref_node(age_id, devices_folder, 0).await?;
    vault.ref_node(age_info, can_visit, 0).await?;
    vault.ref_node(age_info, sdl_node, 0).await?;
    vault.ref_node(age_info, age_owners, 0).await?;
    vault.ref_node(age_info, child_ages, 0).await?;

    if let Some(owner_id) = add_owner {
        vault.ref_node(age_owners, owner_id, 0).await?;
    }

    let display_name = if !description.is_empty() {
        description
    } else if !instance_name.is_empty() {
        instance_name
    } else {
        age_filename
    };
    let game_server = GameServer {
        instance_id: *age_uuid,
        age_filename: age_filename.to_string(),
        display_name: display_name.to_string(),
        age_id,
        sdl_id: sdl_node,
        temporary: false
    };
    vault.add_game_server(game_server).await?;

    Ok((age_id, age_info))
}
