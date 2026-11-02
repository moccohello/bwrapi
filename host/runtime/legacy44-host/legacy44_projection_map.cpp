// Map, regions, tiles and bullets of one frame.
#include "legacy44_projection.h"
#include "legacy44_bwapi_rules.h"
#include "legacy44_split_tiles.h"

#include <BWAPI/BulletType.h>
#include <BWAPI/Position.h>

#include <algorithm>
#include <iostream>

namespace BWRAPI::Legacy44 {

// Terrain, tile regions, region table and walkability do not change within a
// match. They are rewritten only when their inputs change (a new match or a
// table that appears or changes size).
bool GameDataProjection::project_static_map(const BwrFrame& frame, BWAPI::GameData& data,
                                            std::string& error) {
  const std::size_t width = frame.map_width;
  const std::size_t height = frame.map_height;
  const std::size_t tile_count = width * height;
  const BwrTerrain* terrain = frame.terrain;
  const BwrWalkability* walk = frame.walkability;
  const BwrRegions* regions = frame.regions;
  const BwrRegionNeighbors* neighbors = frame.region_neighbors;
  const bool map_size = width != 0 && height != 0 && width <= kMaxMapTiles && height <= kMaxMapTiles;
  const bool tiles = map_size && terrain != nullptr && terrain->rows == tile_count;
  // A column that the engine does not fill yet is unknown in every row.
  const bool buildable = tiles && known(terrain->buildable[0]);
  const bool heights = tiles && known(terrain->ground_height[0]);
  const bool tile_regions = tiles && known(terrain->region[0]) && known(terrain->region2[0]) &&
                            known(terrain->split_mask[0]);
  const bool walkable = map_size && walk != nullptr && width * 4 <= kMaxWalkTiles &&
                        height * 4 <= kMaxWalkTiles && walk->rows == width * 4 * height * 4 &&
                        known(walk->walkable[0]);
  const bool region_rows = regions != nullptr && regions->rows != 0 && regions->rows <= kMaxRegions;

  if (region_rows) data.regionCount = static_cast<int>(regions->rows);

  const std::array<std::uint64_t, 6> key = {
      match_id_,
      (static_cast<std::uint64_t>(width) << 16) | height,
      tiles ? terrain->rows : 0,
      walkable ? walk->rows : 0,
      region_rows ? regions->rows : 0,
      neighbors != nullptr ? neighbors->rows : 0};
  if (static_map_written_ && key == static_map_key_) return true;

  clear_static_map(data);
  // GameData grids are [x][y]; the tables are row-major.
  if (buildable) {
    for (std::size_t x = 0; x < width; ++x) {
      for (std::size_t y = 0; y < height; ++y)
        data.isBuildable[x][y] = terrain->buildable[y * width + x] == 1;
    }
  }
  if (heights) {
    for (std::size_t x = 0; x < width; ++x) {
      for (std::size_t y = 0; y < height; ++y)
        data.getGroundHeight[x][y] = terrain->ground_height[y * width + x];
    }
  }
  if (tile_regions) {
    constexpr std::size_t split_capacity = sizeof(data.mapSplitTilesMiniTileMask)
        / sizeof(data.mapSplitTilesMiniTileMask[0]);
    SplitTileTable splits(kSplitTileIndexCapacity);
    std::vector<std::uint16_t> encoded(tile_count);
    for (std::size_t index = 0; index < tile_count; ++index) {
      if (!splits.encode(terrain->region[index], terrain->region2[index],
                         terrain->split_mask[index], encoded[index])) {
        error = "distinct split-tile patterns exceed 13-bit split index capacity (8192)";
        return false;
      }
    }
    for (std::size_t x = 0; x < width; ++x) {
      for (std::size_t y = 0; y < height; ++y)
        data.mapTileRegionId[x][y] = encoded[y * width + x];
    }
    const std::size_t split_rows = std::min(split_capacity, splits.entries().size());
    for (std::size_t index = 0; index < split_rows; ++index) {
      const auto& split = splits.entries()[index];
      data.mapSplitTilesMiniTileMask[index] = split.mask;
      data.mapSplitTilesRegion1[index] = split.primary;
      data.mapSplitTilesRegion2[index] = split.secondary;
    }
  }
  if (region_rows) {
    static_region_rows_ = regions->rows;
    for (std::size_t index = 0; index < regions->rows; ++index) {
      auto& destination = data.regions[index];
      const auto set = [&](auto& cell, auto value) {
        cell = static_cast<std::decay_t<decltype(cell)>>(value);
      };
      set(destination.id, regions->id[index]);
      set(destination.islandID, regions->group[index]);
      set(destination.center_x, regions->center_x[index]);
      set(destination.center_y, regions->center_y[index]);
      set(destination.leftMost, regions->left[index]);
      set(destination.rightMost, regions->right[index]);
      set(destination.topMost, regions->top[index]);
      set(destination.bottomMost, regions->bottom[index]);
      if (regions->accessible[index] <= 1) set(destination.isAccessible, regions->accessible[index] == 1);
      if (regions->higher_ground[index] <= 1) set(destination.isHigherGround, regions->higher_ground[index] == 1);
      if (known(regions->priority[index])) set(destination.priority, regions->priority[index]);
      const std::uint32_t start = regions->neighbor_start[index];
      const std::uint16_t count = regions->neighbor_count[index];
      if (neighbors != nullptr && known(start) && known(count) && count <= 256 &&
          start <= neighbors->rows && count <= neighbors->rows - start) {
        set(destination.neighborCount, count);
        for (std::size_t neighbor = 0; neighbor < count; ++neighbor)
          destination.neighbors[neighbor] = neighbors->region[start + neighbor];
      }
    }
  }
  if (walkable) {
    const std::size_t columns = width * 4;
    for (std::size_t x = 0; x < columns; ++x) {
      for (std::size_t y = 0; y < height * 4; ++y)
        data.isWalkable[x][y] = walk->walkable[y * columns + x] == 1;
    }
  }
  static_map_written_ = true;
  static_map_key_ = key;
  return true;
}

// Static members are zero unless a completed static write is still present:
// a failed write fails the frame, and the next frame clears all of GameData.
void GameDataProjection::clear_static_map(BWAPI::GameData& data) {
  if (static_map_written_) {
    constexpr std::size_t terrain_begin = offsetof(BWAPI::GameData, getGroundHeight);
    constexpr std::size_t terrain_end = offsetof(BWAPI::GameData, isVisible);
    constexpr std::size_t tiles_begin = offsetof(BWAPI::GameData, mapTileRegionId);
    constexpr std::size_t tiles_end = offsetof(BWAPI::GameData, regionCount);
    zero_game_data(data, terrain_begin, terrain_end - terrain_begin);
    zero_game_data(data, tiles_begin, tiles_end - tiles_begin);
    zero_game_data(data, offsetof(BWAPI::GameData, regions),
                   static_region_rows_ * sizeof(BWAPI::RegionData));
  }
  static_region_rows_ = 0;
  static_map_written_ = false;
}

bool GameDataProjection::project_map(const BwrFrame& frame, const FrameSample& sample,
                                     BWAPI::GameData& data, std::string& error) {
  const std::size_t width = frame.map_width;
  const std::size_t height = frame.map_height;
  if (width == 0 || height == 0 || width > kMaxMapTiles || height > kMaxMapTiles) return true;
  if (!project_static_map(frame, data, error)) return false;

  data.mapWidth = static_cast<int>(width);
  data.mapHeight = static_cast<int>(height);

  const BwrVision* vision = frame.vision;
  const BwrTerrain* terrain = frame.terrain;
  const std::size_t tile_count = width * height;
  const bool has_vision = vision != nullptr && vision->rows == tile_count;
  const bool complete_map = data.flags[BWAPI::Flag::CompleteMapInformation];
  const int self = sample.local_player;
  // A bit p of a vision mask is the view of player p; a replay has every bit.
  const auto seen = [&](std::uint16_t mask) {
    return sample.is_replay || self < 0 ? mask != 0 : ((mask >> self) & 1) != 0;
  };
  // Vision per tile, then creep and occupancy: hidden tiles read false outside
  // a replay or CompleteMapInformation (BWAPI hides them), otherwise the
  // engine flags.
  if (has_vision && (self >= 0 || sample.is_replay)) {
    for (std::size_t x = 0; x < width; ++x) {
      for (std::size_t y = 0; y < height; ++y) {
        const std::size_t index = y * width + x;
        data.isVisible[x][y] = seen(vision->visible[index]);
        data.isExplored[x][y] = seen(vision->explored[index]);
        const bool hidden = !sample.is_replay && !data.isVisible[x][y] && !complete_map;
        if (hidden) {
          data.hasCreep[x][y] = false;
          data.isOccupied[x][y] = false;
          continue;
        }
        const std::uint8_t map_creep = terrain != nullptr && terrain->rows == tile_count
                                           ? terrain->creep[index] : 255;
        const std::uint8_t spread = vision->creep[index];
        if (map_creep == 1 || spread == 1) data.hasCreep[x][y] = true;
        else if (map_creep == 0 && spread == 0) data.hasCreep[x][y] = false;
        if (vision->occupied[index] <= 1) data.isOccupied[x][y] = vision->occupied[index] == 1;
      }
    }
  }
  return true;
}

void GameDataProjection::project_bullets(const BwrFrame& frame, const FrameSample& sample,
                                         BWAPI::GameData& data) {
  const BwrBullets* bullets = frame.bullets;
  if (bullets == nullptr) {
    // No table: the official default, an empty Bulletset, with unknown absence.
    bullet_slots_.clear();
    bullet_stamp_.clear();
    return;
  }
  if (bullets->rows > kMaxBullets) {
    // More bullets than GameData holds: neither known absence
    // nor evidence that cached lifetimes ended.
    if (!warned_bullets_) {
      std::cerr << "[bullet-unavailable] frame=" << sample.frame << " count=" << bullets->rows
                << " capacity=" << kMaxBullets << '\n';
      warned_bullets_ = true;
    }
    return;
  }

  // GameBullets.cpp updateBullets()/BulletImpl::saveExists(): a slot keeps its
  // ID while present in consecutive frames and gets nextId++ when it was
  // absent in the previous frame. The engine id is slot + 1; the stamp names
  // the frame a slot was last seen in.
  ++bullet_epoch_;
  const auto set = [&](auto& cell, auto value) {
    cell = static_cast<std::decay_t<decltype(cell)>>(value);
  };
  const bool complete_map = data.flags[BWAPI::Flag::CompleteMapInformation];
  const auto bullet_slot = [&](std::uint64_t id) -> std::size_t {
    return static_cast<std::size_t>(id - 1);
  };
  for (std::size_t row = 0; row < bullets->rows; ++row) {
    const std::uint64_t engine_id = bullets->id[row];
    const int type = bullets->bullet_type[row];
    if (type >= BWAPI::BulletTypes::Enum::MAX || engine_id == 0) continue;
    const auto slot = bullet_slot(engine_id);
    if (slot >= bullet_slots_.size()) {
      bullet_slots_.resize(slot + 1);
      bullet_stamp_.resize(slot + 1, 0);
    }
    BulletSlot& cached = bullet_slots_[slot];
    if (!(bullet_stamp_[slot] == bullet_epoch_ - 1 && cached.type == type)) {
      cached.id = next_bullet_id_++;
      cached.type = type;
    }
    bullet_stamp_[slot] = bullet_epoch_;

    auto& destination = data.bullets[row];
    // BulletImpl::updateData: every cell 0..8 of a player with an ID is one
    // value, true under CompleteMapInformation and otherwise whether the
    // map tile under the bullet is visible (Game::isVisible, the BWAPI
    // player's view or any player's in a replay); a bullet without a sprite
    // (unknown sprite bits) is seen by nobody. The bullet is accessible
    // (exists, with real data) only under CompleteMapInformation, in a
    // replay, or while Bullet::isVisible() holds; otherwise it keeps the
    // official defaults.
    const int tile_x = bullets->x[row] / 32;
    const int tile_y = bullets->y[row] / 32;
    const bool has_sprite = known(bullets->visible_to[row]);
    const bool tile_visible = bullets->x[row] >= 0 && bullets->y[row] >= 0 &&
                              tile_x < data.mapWidth && tile_y < data.mapHeight &&
                              data.isVisible[tile_x][tile_y];
    const bool visible = has_sprite && (complete_map || tile_visible);
    std::fill(std::begin(destination.isVisible), std::end(destination.isVisible), false);
    for (int player = 0; player < 9; ++player)
      destination.isVisible[player] = visible && player_ids_.slot(player) >= 0;
    const int self = player_ids_.id(sample.local_player);
    const bool self_sees = self >= 0 && self < 9
                               ? destination.isVisible[self]
                               : std::any_of(std::begin(destination.isVisible),
                                             std::end(destination.isVisible),
                                             [](bool cell) { return cell; });
    if (!(complete_map || sample.is_replay || self_sees)) {
      destination.id = -1;
      destination.player = -1;
      destination.type = BWAPI::BulletTypes::Enum::Unknown;
      destination.source = -1;
      destination.positionX = BWAPI::Positions::Unknown.x;
      destination.positionY = BWAPI::Positions::Unknown.y;
      destination.angle = 0;
      destination.velocityX = 0;
      destination.velocityY = 0;
      destination.target = -1;
      destination.targetPositionX = BWAPI::Positions::Unknown.x;
      destination.targetPositionY = BWAPI::Positions::Unknown.y;
      destination.removeTimer = 0;
      destination.exists = false;
      continue;
    }
    set(destination.id, cached.id);
    // BulletImpl writes the owner's PlayerImpl::getID(), which only the
    // playable players of the set have; the neutral player's is -1.
    destination.player = -1;
    if (known(bullets->owner[row]) && bullets->owner[row] != kNeutralPlayerId)
      destination.player = player_ids_.listed(bullets->owner[row]);
    set(destination.type, type);
    set(destination.positionX, bullets->x[row]);
    set(destination.positionY, bullets->y[row]);
    // Source and target name a unit only while it exists (is published).
    const auto relation = [&](int& cell, std::uint64_t id) {
      cell = -1;
      if (id == 0 || !known(id)) return;
      const auto unit_slot = engine_slot(id);
      if (unit_slot < slot_life_.size() && slot_life_[unit_slot] == id &&
          unit_slot < frame_access_.size() && frame_access_[unit_slot] == 2)
        cell = slot_unit_id_[unit_slot];
    };
    relation(destination.source, bullets->source[row]);
    relation(destination.target, bullets->target[row]);
    destination.targetPositionX = destination.targetPositionY = -1;
    if (known(bullets->target_x[row]) && known(bullets->target_y[row])) {
      set(destination.targetPositionX, bullets->target_x[row]);
      set(destination.targetPositionY, bullets->target_y[row]);
    }
    if (known(bullets->remove_timer[row])) set(destination.removeTimer, bullets->remove_timer[row]);
    if (known(bullets->velocity_x[row])) set(destination.velocityX, bullets->velocity_x[row] / 256.0);
    if (known(bullets->velocity_y[row])) set(destination.velocityY, bullets->velocity_y[row] / 256.0);
    if (known(bullets->direction[row])) set(destination.angle, direction_to_angle(bullets->direction[row]));
    set(destination.exists, true);
  }
}

}  // namespace BWRAPI::Legacy44
