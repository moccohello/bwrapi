// BWAPI events derived from consecutive engine frames and the match lifecycle.
#include "legacy44_test_support.h"
#include "legacy44_events.h"

#include <BWAPI/Flag.h>
#include <BWAPI/Order.h>
#include <BWAPI/Position.h>

#include <cstring>
#include <vector>

using namespace BWRAPI::Legacy44;
using namespace BWRAPI::Legacy44::Test;
namespace UT = BWAPI::UnitTypes::Enum;
namespace ET = BWAPI::EventType;

namespace {

struct Run {
  Env env;
  EventFrameAdapter events;

  // Projects the fixture's frame and appends its events.
  bool step() {
    if (!env.project()) return false;
    env.data->eventCount = 0;
    return events.append(env.sample, env.fx.build(), *env.data, env.error);
  }
  std::vector<std::pair<int, int>> list() const {
    std::vector<std::pair<int, int>> result;
    for (int i = 0; i < env.data->eventCount; ++i)
      result.push_back({static_cast<int>(env.data->events[i].type), env.data->events[i].v1});
    return result;
  }
  bool has(int type, int v1) const {
    for (const auto& event : list())
      if (event.first == type && event.second == v1) return true;
    return false;
  }
  int count(int type) const {
    int found = 0;
    for (const auto& event : list()) found += event.first == type;
    return found;
  }
  int index_of(int type, int v1) const {
    const auto events_list = list();
    for (std::size_t i = 0; i < events_list.size(); ++i)
      if (events_list[i].first == type && events_list[i].second == v1) return static_cast<int>(i);
    return -1;
  }
};

void first_frame_of_a_match() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10),
                           unit_row(2, UT::Terran_Marine, 1, 20, 20)};
  run.env.fx.render.rows.resize(2);
  run.env.fx.render.rows[0] = {unit_id(1), 10, 10, 0b01};
  run.env.fx.render.rows[1] = {unit_id(2), 20, 20, 0b00};
  CHECK(run.step());
  const int a = run.env.sample.units[0].id;
  const auto first = run.list();
  CHECK(!first.empty() && first.front().first == ET::MatchStart && first.back().first == ET::MatchFrame);
  // The visible own unit is created, discovered, completed and shown; the
  // hidden enemy stays alive but inaccessible, so it gets no event at all.
  CHECK(run.has(ET::UnitCreate, a) && run.has(ET::UnitDiscover, a));
  CHECK(run.has(ET::UnitComplete, a) && run.has(ET::UnitShow, a));
  CHECK(run.count(ET::UnitCreate) == 1 && run.count(ET::UnitDiscover) == 1);
  // Both live units entered the first update's unit vector.
  CHECK(run.env.data->initialUnitCount == 2);
  CHECK(run.events.match_active());
  // The next frame only has MatchFrame.
  CHECK(run.step());
  CHECK(run.count(ET::MatchStart) == 0 && run.list().size() == 1 && run.count(ET::MatchFrame) == 1);
}

// A hidden enemy that later becomes visible is only discovered and shown; a
// unit that first exists in view is also created (computePrimaryUnitSets).
void created_and_discovered_units() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10),
                           unit_row(3, UT::Terran_Marine, 1, 30, 30)};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(3), 30, 30, 0b00}};
  CHECK(run.step());
  CHECK(run.count(ET::UnitDiscover) == 1 && run.count(ET::UnitCreate) == 1);
  run.env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10),
                           unit_row(2, UT::Terran_Marine, 0, 20, 20),   // own: created
                           unit_row(3, UT::Terran_Marine, 1, 30, 30)};  // hidden so far
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(2), 20, 20, 0b01},
                            {unit_id(3), 30, 30, 0b00}};
  CHECK(run.step());
  const int own = run.env.sample.units[1].id;
  CHECK(run.has(ET::UnitCreate, own) && run.has(ET::UnitDiscover, own));
  // Create comes before Discover, MatchFrame last.
  CHECK(run.index_of(ET::UnitCreate, own) < run.index_of(ET::UnitDiscover, own));
  CHECK(run.list().back().first == ET::MatchFrame);
  // The enemy steps into view: discovered and shown, never created.
  run.env.fx.render.rows[2] = {unit_id(3), 30, 30, 0b01};
  CHECK(run.step());
  const int other = run.env.sample.units[2].id;
  CHECK(!run.has(ET::UnitCreate, other) && run.has(ET::UnitDiscover, other));
  CHECK(run.has(ET::UnitShow, other) && run.count(ET::UnitDiscover) == 1);
}

void show_and_hide() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 1, 10, 10)};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(run.step());
  const int id = run.env.sample.units[0].id;
  // Hidden from the start: alive and tracked, but no event at all.
  CHECK(run.count(ET::UnitDiscover) == 0 && run.count(ET::UnitShow) == 0);
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}};
  CHECK(run.step());
  CHECK(run.has(ET::UnitDiscover, id) && run.has(ET::UnitShow, id));
  CHECK(run.count(ET::UnitHide) == 0);
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}};
  CHECK(run.step());
  CHECK(run.count(ET::UnitShow) == 0 && run.count(ET::UnitHide) == 0);
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(run.step());
  CHECK(run.has(ET::UnitHide, id) && run.has(ET::UnitEvade, id));
}

// A unit that loses visibility keeps existing but is evaded; coming back is
// another discover with no create (computePrimaryUnitSets else branch).
void evade_and_rediscover() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 1, 10, 10)};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}};
  CHECK(run.step());
  const int id = run.env.sample.units[0].id;
  CHECK(run.env.unit(id).exists);
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(run.step());
  CHECK(run.has(ET::UnitHide, id) && run.has(ET::UnitEvade, id));
  CHECK(!run.has(ET::UnitDestroy, id) && !run.env.unit(id).exists);
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}};
  CHECK(run.step());
  CHECK(run.has(ET::UnitDiscover, id) && run.has(ET::UnitShow, id));
  CHECK(!run.has(ET::UnitCreate, id) && !run.has(ET::UnitEvade, id));
  CHECK(run.env.unit(id).exists);
}

// CompleteMapInformation publishes the hidden enemy the moment it is on.
void complete_map_information_publishes_hidden_units() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 1, 10, 10)};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(run.step());
  const int id = run.env.sample.units[0].id;
  CHECK(!run.env.unit(id).exists);
  run.env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(run.step());
  CHECK(run.env.sample.units.size() == 1 && run.env.unit(id).exists);
  CHECK(run.has(ET::UnitDiscover, id) && !run.has(ET::UnitCreate, id));
}

// A unit missing from the table is dead: hide (if shown), evade, destroy.
void destroyed_units() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 1, 10, 10),
                           unit_row(2, UT::Zerg_Zergling, 0, 20, 20)};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(2), 20, 20, 0b01}};
  CHECK(run.step());
  const int enemy = run.env.sample.units[0].id;
  const int own = run.env.sample.units[1].id;
  run.env.fx.units.rows = {unit_row(2, UT::Zerg_Zergling, 0, 20, 20)};
  run.env.fx.render.rows = {{unit_id(2), 20, 20, 0b01}};
  CHECK(run.step());
  CHECK(run.has(ET::UnitHide, enemy) && run.has(ET::UnitEvade, enemy) && run.has(ET::UnitDestroy, enemy));
  CHECK(run.index_of(ET::UnitHide, enemy) < run.index_of(ET::UnitEvade, enemy));
  CHECK(run.index_of(ET::UnitEvade, enemy) < run.index_of(ET::UnitDestroy, enemy));
  CHECK(!run.env.unit(enemy).exists && run.env.unit(own).exists);
  // A destroyed unit is reported once.
  CHECK(run.step());
  CHECK(run.count(ET::UnitDestroy) == 0);
}

void morph_complete_and_renegade() {
  Run run;
  run.env.fx.local_player = 0;
  auto egg = unit_row(1, UT::Zerg_Egg, 0, 10, 10);
  egg.completed = 1;
  auto building = unit_row(2, UT::Protoss_Pylon, 1, 30, 30);
  building.completed = 0;
  run.env.fx.units.rows = {egg, building};
  run.env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(2), 30, 30, 0b01}};
  CHECK(run.step());
  const int egg_id = run.env.sample.units[0].id;
  const int pylon = run.env.sample.units[1].id;
  auto hatched = unit_row(1, UT::Zerg_Zergling, 0, 10, 10);
  hatched.completed = 0;
  building.completed = 1;
  building.owner = 0;  // traded
  run.env.fx.units.rows = {hatched, building};
  CHECK(run.step());
  CHECK(run.has(ET::UnitMorph, egg_id) && run.has(ET::UnitRenegade, pylon));
  CHECK(run.has(ET::UnitComplete, pylon) && !run.has(ET::UnitComplete, egg_id));
  // Morph and Renegade come after the primary events.
  CHECK(run.index_of(ET::UnitMorph, egg_id) > run.index_of(ET::UnitComplete, pylon));
  // The morphed unit completes later.
  hatched.completed = 1;
  run.env.fx.units.rows = {hatched, building};
  CHECK(run.step());
  CHECK(run.has(ET::UnitComplete, egg_id));
}

// PlayerLeft names the player's ID: slot 3 after the empty slots 1 and 2 is ID 1.
void player_left() {
  Run run;
  run.env.fx.local_player = 0;
  auto a = player_row(0, 0);
  auto b = player_row(3, 1);
  auto empty = player_row(1, 0);
  empty.player_type = BWAPI::PlayerTypes::Enum::None;
  auto closed = player_row(2, 0);
  closed.player_type = BWAPI::PlayerTypes::Enum::Closed;
  a.left_game = 0;
  b.left_game = 0;
  run.env.fx.players.rows = {a, empty, closed, b};
  CHECK(run.step());
  CHECK(run.count(ET::PlayerLeft) == 0);
  run.env.fx.players.rows[3].left_game = 1;
  CHECK(run.step());
  CHECK(run.has(ET::PlayerLeft, 1) && run.count(ET::PlayerLeft) == 1);
  CHECK(run.step());
  CHECK(run.count(ET::PlayerLeft) == 0);
}

// The match ends with MatchFrame + MatchEnd(winner), then MenuFrame.
void match_end() {
  Run run;
  run.env.fx.local_player = 0;
  auto me = player_row(0, 0);
  me.victory = 3;
  run.env.fx.players.rows = {me};
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Drone, 0, 10, 10)};
  CHECK(run.step());
  CHECK(run.events.stage_match_end(*run.env.data, run.env.error));
  CHECK(run.env.data->isInGame && run.env.data->eventCount == 2);
  CHECK(run.env.data->events[0].type == ET::MatchFrame);
  CHECK(run.env.data->events[1].type == ET::MatchEnd && run.env.data->events[1].v1 == 1);
  bool advanced = false;
  run.env.projection.leave_match(*run.env.data);
  CHECK(run.events.advance_pending_lifecycle(*run.env.data, advanced, run.env.error) && advanced);
  CHECK(!run.env.data->isInGame && run.env.data->eventCount == 1 &&
        run.env.data->events[0].type == ET::MenuFrame);
  CHECK(!run.events.match_active());
  // A new match starts over, with its own MatchStart and Create events.
  run.env.fx.match_id = 2;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Drone, 0, 10, 10, 2)};
  CHECK(run.step());
  CHECK(run.count(ET::MatchStart) == 1 && run.count(ET::UnitCreate) == 1);
  // BWAPI UnitIDs restart from zero with the new match's unit vector.
  CHECK(run.env.data->initialUnitCount == 1 && run.env.sample.units[0].id == 0);
}

void match_end_without_a_win() {
  Run run;
  run.env.fx.local_player = 0;
  run.env.fx.players.rows = {player_row(0, 0)};
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Drone, 0, 10, 10)};
  CHECK(run.step());
  CHECK(run.events.stage_match_end(*run.env.data, run.env.error));
  CHECK(run.env.data->events[1].v1 == 0);
}

void a_new_match_needs_the_boundary_first() {
  Run run;
  run.env.fx.units.rows = {unit_row(1, UT::Zerg_Drone, 0, 10, 10)};
  CHECK(run.step());
  run.env.fx.match_id = 2;
  CHECK(!run.step());
}

void menu_and_paused_frames() {
  Run run;
  CHECK(run.events.stage_menu_frame(*run.env.data, run.env.error));
  CHECK(run.env.data->eventCount == 1 && run.env.data->events[0].type == ET::MenuFrame);
  CHECK(run.events.stage_match_frame(*run.env.data, run.env.error));
  CHECK(run.env.data->eventCount == 1 && run.env.data->events[0].type == ET::MatchFrame);
}

}  // namespace

// GameUnits.cpp extractUnitData: the first NukeTrack frame of a Ghost raises
// NukeDetect at its painted target; the target is a nuke dot while visible.
// The game goes from CastNuclearStrike straight to NukeTrack, so that order's
// target counts as painted too.
void nuke_detect() {
  Run run;
  run.env.fx.local_player = 0;
  auto ghost = unit_row(1, UT::Terran_Ghost, 0, 10, 10);
  ghost.order = BWAPI::Orders::Enum::CastNuclearStrike;
  ghost.order_target_x = 640;
  ghost.order_target_y = 960;
  run.env.fx.units.rows = {ghost};
  CHECK(run.step());
  CHECK(run.count(ET::NukeDetect) == 0 && run.env.data->nukeDotCount == 0);
  // Without vision of the target the event names Positions::Unknown.
  run.env.fx.units.rows[0].order = BWAPI::Orders::Enum::NukeTrack;
  run.env.fx.units.rows[0].order_target_x = 0;
  CHECK(run.step());
  CHECK(run.has(ET::NukeDetect, BWAPI::Positions::Unknown.x) && run.env.data->nukeDotCount == 0);
  // Under CompleteMapInformation the painted target is the dot; the event
  // is raised once per track.
  run.env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(run.step());
  CHECK(run.count(ET::NukeDetect) == 0);
  CHECK(run.env.data->nukeDotCount == 1 && run.env.data->nukeDots[0].x == 640 &&
        run.env.data->nukeDots[0].y == 960);
  run.env.fx.units.rows[0].order = BWAPI::Orders::Enum::Stop;
  CHECK(run.step());
  CHECK(run.env.data->nukeDotCount == 0);
  run.env.fx.units.rows[0].order = BWAPI::Orders::Enum::NukeTrack;
  CHECK(run.step());
  const int index = run.index_of(ET::NukeDetect, 640);
  CHECK(index >= 0 && run.env.data->events[index].v2 == 960);
}

// Chat lines the in-game DLL took: a received line of another player comes
// before the unit events while user input is on, a sent line after them; the
// local player's own recorded line and a player outside the match's set give
// nothing.
void chat_lines() {
  Run run;
  run.env.fx.local_player = 0;
  for (int slot : {0, 1}) run.env.fx.players.rows.push_back(player_row(slot, 1));
  run.env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10)};
  run.env.fx.render.rows.resize(1);
  run.env.fx.render.rows[0] = {unit_id(1), 10, 10, 0b01};
  CHECK(run.step());
  const auto chat_step = [&run] {
    if (!run.env.project()) return false;
    run.env.data->eventCount = 0;
    run.env.data->eventStringCount = 0;
    run.env.data->self = 0;
    return run.events.append(run.env.sample, run.env.fx.build(), *run.env.data, run.env.error);
  };
  run.events.queue_chat(false, 1, "hello");
  CHECK(chat_step());
  CHECK(run.count(ET::ReceiveText) == 0);

  run.env.data->flags[BWAPI::Flag::UserInput] = true;
  run.env.fx.units.rows.push_back(unit_row(2, UT::Terran_Marine, 0, 30, 30));
  run.env.fx.render.rows.push_back({unit_id(2), 30, 30, 0b01});
  run.events.queue_chat(true, 0xffffffffu, "gg");
  run.events.queue_chat(false, 1, "hello");
  run.events.queue_chat(false, 0, "own line");
  run.events.queue_chat(false, 7, "outside the match");
  CHECK(chat_step());
  const auto events = run.list();
  int received = -1, sent = -1;
  for (int i = 0; i < static_cast<int>(events.size()); ++i) {
    if (events[i].first == ET::ReceiveText) received = i;
    if (events[i].first == ET::SendText) sent = i;
  }
  const int created = run.index_of(ET::UnitCreate, run.env.sample.units[1].id);
  CHECK(run.count(ET::ReceiveText) == 1 && run.count(ET::SendText) == 1);
  CHECK(received >= 0 && created > received && sent > created);
  CHECK(run.env.data->events[received].v1 == 1);
  CHECK(std::strcmp(run.env.data->eventStrings[run.env.data->events[received].v2], "hello") == 0);
  CHECK(std::strcmp(run.env.data->eventStrings[run.env.data->events[sent].v1], "gg") == 0);

  // A paused round carries the lines too; the next frame has none left.
  run.events.queue_chat(true, 0xffffffffu, "paused");
  CHECK(run.events.stage_match_frame(*run.env.data, run.env.error));
  CHECK(run.count(ET::SendText) == 1 && run.list().back().first == ET::MatchFrame);
  CHECK(chat_step());
  CHECK(run.count(ET::SendText) == 0 && run.count(ET::ReceiveText) == 0);
}

// A save the local player made comes as SaveGame with its name, ahead of the
// frame's other events, and in a paused round too.
void saves() {
  Run run;
  run.env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10)};
  run.env.fx.render.rows.resize(1);
  run.env.fx.render.rows[0] = {unit_id(1), 10, 10, 0b01};
  CHECK(run.step());
  run.env.fx.units.rows.push_back(unit_row(2, UT::Terran_Marine, 0, 30, 30));
  run.env.fx.render.rows.push_back({unit_id(2), 30, 30, 0b01});
  run.events.queue_save("first save");
  CHECK(run.env.project());
  run.env.data->eventCount = 0;
  run.env.data->eventStringCount = 0;
  CHECK(run.events.append(run.env.sample, run.env.fx.build(), *run.env.data, run.env.error));
  const int saved = run.index_of(ET::SaveGame, 0);
  const int created = run.index_of(ET::UnitCreate, run.env.sample.units[1].id);
  CHECK(run.count(ET::SaveGame) == 1 && saved >= 0 && created > saved);
  CHECK(std::strcmp(run.env.data->eventStrings[run.env.data->events[saved].v1], "first save") == 0);
  run.events.queue_save("paused save");
  CHECK(run.events.stage_match_frame(*run.env.data, run.env.error));
  CHECK(run.count(ET::SaveGame) == 1 && run.list().back().first == ET::MatchFrame);
  CHECK(std::strcmp(run.env.data->eventStrings[run.env.data->events[0].v1], "paused save") == 0);
  CHECK(run.step());
  CHECK(run.count(ET::SaveGame) == 0);
}

int main() {
  chat_lines();
  saves();
  first_frame_of_a_match();
  created_and_discovered_units();
  show_and_hide();
  evade_and_rediscover();
  complete_map_information_publishes_hidden_units();
  destroyed_units();
  morph_complete_and_renegade();
  player_left();
  match_end();
  match_end_without_a_win();
  a_new_match_needs_the_boundary_first();
  menu_and_paused_frames();
  nuke_detect();
  return finish();
}
