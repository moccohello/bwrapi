// Map tiles, regions, walkability and bullets built from the engine tables.
#include "legacy44_test_support.h"

#include <BWAPI/BulletType.h>
#include <BWAPI/Position.h>

using namespace BWRAPI::Legacy44;
using namespace BWRAPI::Legacy44::Test;

namespace {

// A width x height map whose tile (x, y) is buildable when x is even, has
// height y % 3, and lies in region 1 (x < width / 2) or 2.
void fill_map(Env& env, unsigned width, unsigned height) {
  env.fx.map_width = static_cast<std::uint16_t>(width);
  env.fx.map_height = static_cast<std::uint16_t>(height);
  for (unsigned y = 0; y < height; ++y) {
    for (unsigned x = 0; x < width; ++x) {
      Fixture::TerrainRows::Row tile;
      tile.buildable = x % 2 == 0;
      tile.ground_height = static_cast<std::uint8_t>(y % 3);
      tile.creep = x == 0 && y == 0;
      tile.region = x < width / 2 ? 1 : 2;
      tile.region2 = tile.region;
      tile.split_mask = 0;
      env.fx.terrain.rows.push_back(tile);
      Fixture::VisionRows::Row view;
      view.visible = y == 0 ? 0b01 : 0;
      view.explored = x == 0 ? 0b01 : 0;
      view.creep = 0;
      view.occupied = x == 1 && y == 1;
      env.fx.vision.rows.push_back(view);
    }
  }
  for (unsigned y = 0; y < height * 4; ++y) {
    for (unsigned x = 0; x < width * 4; ++x) {
      Fixture::WalkabilityRows::Row walk;
      walk.walkable = x == 5 && y == 6;
      env.fx.walkability.rows.push_back(walk);
    }
  }
}

void tiles_are_transposed_into_the_grids() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 4, 3);
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.mapWidth == 4 && d.mapHeight == 3);
  CHECK(d.isBuildable[0][0] && !d.isBuildable[1][0] && d.isBuildable[2][2]);
  CHECK(d.getGroundHeight[3][1] == 1 && d.getGroundHeight[2][2] == 2);
  CHECK(d.mapTileRegionId[0][0] == 1 && d.mapTileRegionId[3][2] == 2);
  CHECK(d.isWalkable[5][6] && !d.isWalkable[6][5]);
  // The local player sees row 0 and has explored column 0.
  CHECK(d.isVisible[2][0] && !d.isVisible[2][1]);
  CHECK(d.isExplored[0][2] && !d.isExplored[1][2]);
  // Creep: the map's own creep on a visible tile, known false elsewhere
  // visible; hidden tiles read false.
  CHECK(d.hasCreep[0][0] && !d.hasCreep[1][0]);
  CHECK(!d.isOccupied[1][1]);  // hidden tile
}

void occupied_on_a_visible_tile() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 2, 2);
  env.fx.vision.rows[3].visible = 0b01;  // tile (1, 1)
  CHECK(env.project());
  CHECK(env.data->isOccupied[1][1]);
}

void replay_sees_every_bit() {
  Env env;
  env.fx.flags = BWR_FRAME_REPLAY;
  fill_map(env, 2, 2);
  env.fx.vision.rows[3].visible = 0b10000;
  CHECK(env.project());
  CHECK(env.data->isVisible[1][1] && env.data->isVisible[1][0]);
  CHECK(env.data->isOccupied[1][1]);
}

// A tile split between two regions is encoded as an index into the split table.
void split_tiles() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 2, 1);
  env.fx.terrain.rows[1].region = 3;
  env.fx.terrain.rows[1].region2 = 4;
  env.fx.terrain.rows[1].split_mask = 0x00ff;
  CHECK(env.project());
  const auto& d = *env.data;
  const unsigned encoded = d.mapTileRegionId[1][0];
  CHECK((encoded & 0x2000) != 0);
  const unsigned index = encoded & 0x1fff;
  CHECK(d.mapSplitTilesRegion1[index] == 3 && d.mapSplitTilesRegion2[index] == 4);
  CHECK(d.mapSplitTilesMiniTileMask[index] == 0x00ff);
}

void regions_and_neighbors() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 2, 1);
  Fixture::RegionsRows::Row a, b;
  a.id = 1;
  a.group = 7;
  a.center_x = 100;
  a.center_y = 200;
  a.left = 0;
  a.top = 1;
  a.right = 50;
  a.bottom = 60;
  a.accessible = 1;
  a.higher_ground = 0;
  a.priority = 4;
  a.neighbor_start = 0;
  a.neighbor_count = 2;
  b = a;
  b.id = 2;
  b.neighbor_start = 2;
  b.neighbor_count = 1;
  env.fx.regions.rows = {a, b};
  for (std::uint16_t region : {2, 3, 1}) {
    Fixture::RegionNeighborsRows::Row n;
    n.region = region;
    env.fx.region_neighbors.rows.push_back(n);
  }
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.regionCount == 2 && d.regions[0].id == 1 && d.regions[1].id == 2);
  CHECK(d.regions[0].islandID == 7 && d.regions[0].center_x == 100 && d.regions[0].bottomMost == 60);
  CHECK(d.regions[0].isAccessible && !d.regions[0].isHigherGround && d.regions[0].priority == 4);
  CHECK(d.regions[0].neighborCount == 2 && d.regions[0].neighbors[0] == 2 && d.regions[0].neighbors[1] == 3);
  CHECK(d.regions[1].neighborCount == 1 && d.regions[1].neighbors[0] == 1);
}

// The static members are written once per match: a later frame of the same
// match keeps them even if the shared tables are later missing.
void static_map_follows_the_match() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 2, 2);
  CHECK(env.project());
  CHECK(env.data->isBuildable[0][0]);
  env.fx.terrain.rows[0].buildable = 0;
  CHECK(env.project());
  CHECK(env.data->isBuildable[0][0]);  // same match: not rewritten
  env.fx.match_id = 2;
  CHECK(env.project());
  CHECK(!env.data->isBuildable[0][0]);  // new match
}

void leaving_the_match_clears_the_map() {
  Env env;
  env.fx.local_player = 0;
  fill_map(env, 2, 2);
  CHECK(env.project());
  CHECK(env.data->isBuildable[0][0] && env.data->isInGame);
  env.projection.leave_match(*env.data);
  CHECK(!env.data->isBuildable[0][0] && !env.data->isInGame && env.data->mapWidth == 0);
}

void bullets() {
  Env env;
  env.fx.local_player = 2;
  // Slots 2 and 3 are IDs 0 and 1; the neutral player is ID 2.
  env.fx.players.rows = {player_row(2, 0), player_row(3, 0)};
  env.fx.units.rows = {unit_row(1, 0, 2, 10, 10), unit_row(2, 0, 3, 20, 20)};
  // Both units are published: source and target name only those.
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b1100}, {unit_id(2), 20, 20, 0b1100}};
  Fixture::BulletsRows::Row shot;
  shot.id = 8;  // slot 7
  shot.bullet_type = BWAPI::BulletTypes::Enum::Gauss_Rifle_Hit;
  shot.owner = 2;
  shot.x = 30;
  shot.y = 40;
  shot.source = unit_id(1);
  shot.target = unit_id(2);
  shot.remove_timer = 9;
  shot.visible_to = 0b100;
  shot.target_x = 25;
  shot.target_y = 26;
  shot.velocity_x = 256;
  shot.direction = 64;
  env.fx.bullets.rows = {shot};
  // The bullet stands on tile (0, 1), which the local player sees.
  fill_map(env, 2, 2);
  env.fx.vision.rows[2].visible = 0b100;
  CHECK(env.project());
  const auto& bullet = env.data->bullets[0];
  CHECK(bullet.exists && bullet.type == BWAPI::BulletTypes::Enum::Gauss_Rifle_Hit);
  CHECK(bullet.player == 0 && bullet.positionX == 30 && bullet.removeTimer == 9);
  CHECK(bullet.source == env.sample.units[0].id && bullet.target == env.sample.units[1].id);
  CHECK(bullet.targetPositionX == 25 && bullet.velocityX == 1.0 && bullet.angle == 0.0);
  // Cells of IDs without a player stay false.
  CHECK(bullet.isVisible[0] && bullet.isVisible[2] && !bullet.isVisible[3] && !bullet.isVisible[8]);
  const int id = bullet.id;
  // Consecutive frames keep the id; a gap or a type change gives a new one.
  CHECK(env.project());
  CHECK(env.data->bullets[0].id == id);
  env.fx.bullets.rows.clear();
  CHECK(env.project());
  CHECK(!env.data->bullets[0].exists);
  env.fx.bullets.rows = {shot};
  CHECK(env.project());
  CHECK(env.data->bullets[0].id == id + 1);
  env.fx.bullets.rows[0].bullet_type = BWAPI::BulletTypes::Enum::Burst_Lasers;
  CHECK(env.project());
  CHECK(env.data->bullets[0].id == id + 2);
}

// A bullet is visible while the local player sees the tile under it, to
// every player at once, and exists only then or under
// CompleteMapInformation; its own sprite bits do not matter.
void bullet_visibility_rules() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows = {player_row(0, 0), player_row(1, 0), player_row(2, 0), player_row(3, 0)};
  fill_map(env, 2, 2);  // the local player sees row 0
  Fixture::BulletsRows::Row shot;
  shot.id = 1;
  shot.bullet_type = BWAPI::BulletTypes::Enum::Gauss_Rifle_Hit;
  shot.x = 10;
  shot.y = 10;
  shot.visible_to = 0b10;
  env.fx.bullets.rows = {shot};
  CHECK(env.project());
  CHECK(env.data->bullets[0].exists && env.data->bullets[0].isVisible[0] && env.data->bullets[0].isVisible[3]);
  env.fx.bullets.rows[0].y = 40;  // tile (0, 1) is hidden
  CHECK(env.project());
  const auto& hidden = env.data->bullets[0];
  CHECK(!hidden.exists && !hidden.isVisible[0] && hidden.id == -1);
  CHECK(hidden.type == BWAPI::BulletTypes::Enum::Unknown && hidden.positionX == BWAPI::Positions::Unknown.x);
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(env.project());
  CHECK(env.data->bullets[0].exists && env.data->bullets[0].isVisible[3]);
  // Without a sprite nobody sees it, but CompleteMapInformation still
  // publishes it.
  env.fx.bullets.rows[0].visible_to = 0xffffffffu;
  CHECK(env.project());
  CHECK(env.data->bullets[0].exists && !env.data->bullets[0].isVisible[0]);
}

void no_bullet_table() {
  Env env;
  env.fx.units.rows = {unit_row(1, 0, 0, 10, 10)};
  CHECK(env.project());
  CHECK(!env.data->bullets[0].exists);
  Env empty;
  empty.fx.carry = BWR_SOURCE_BULLETS;
  CHECK(empty.project());
  CHECK(!empty.data->bullets[0].exists);
}

}  // namespace

int main() {
  tiles_are_transposed_into_the_grids();
  occupied_on_a_visible_tile();
  replay_sees_every_bit();
  split_tiles();
  regions_and_neighbors();
  static_map_follows_the_match();
  leaving_the_match_clears_the_map();
  bullets();
  bullet_visibility_rules();
  no_bullet_table();
  return finish();
}
