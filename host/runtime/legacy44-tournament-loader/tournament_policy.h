#pragma once

#include <BWAPI.h>
#include <BWAPI/Client.h>

#include <string>
#include <vector>

namespace BWRAPI::Legacy44Tournament {

struct CallbackBoundary {
  int command_count = 0;
  int shape_count = 0;
  int string_count = 0;
  int unit_command_count = 0;
  bool has_lat_com = true;
  bool has_gui = true;
};

// Commands produced while TournamentModule::onAction is active, and the
// immediately-authorized originating command, must not be submitted to the
// same policy a second time after the regular AIModule callback returns.
struct ImmediateActionTrace {
  void clear();
  void remember_command(int index);
  bool contains_command(int index) const;

 private:
  std::vector<int> command_indices_;
};

void capture_boundary(const BWAPI::GameData& data, CallbackBoundary& boundary);

// Invokes the pinned onAction path while preserving output emitted by the
// TournamentModule itself. `allowed` is the controller's decision.
bool authorize_immediate_action(BWAPI::GameData& data,
                                BWAPI::TournamentModule& controller,
                                BWAPI::Tournament::ActionID action,
                                void* parameter,
                                ImmediateActionTrace& trace,
                                bool& allowed,
                                std::string& error);

// Applies the pinned BWAPI 4.4 server-side tournamentCheck policy to only the
// general commands appended by the regular AIModule callback. Commands emitted
// by TournamentModule::onAction are retained without recursive policy checks,
// matching GameImpl::isTournamentCall. Unit commands have no Tournament ActionID.
bool gate_regular_output(BWAPI::GameData& data, const CallbackBoundary& boundary,
                         BWAPI::TournamentModule& controller,
                         const ImmediateActionTrace& immediate_actions,
                         std::string& error);

}  // namespace BWRAPI::Legacy44Tournament
