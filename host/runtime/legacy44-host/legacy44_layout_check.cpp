#include <cstddef>

#include <BWAPI/Client/GameData.h>
#include <BWAPI/Client/GameTable.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

// The host and the official Win32 loader both compile the upstream headers.
// These checks cover the sizes and offsets of the GameData the host shares
// with the client, so a header change that moves them fails the build.
static_assert(sizeof(BWAPI::GameInstance) == 12,
              "BWAPI 4.4 GameInstance layout is not the expected ABI");
static_assert(sizeof(BWAPI::GameTable) == 96,
              "BWAPI 4.4 GameTable layout is not the expected ABI");
static_assert(BWAPI::UnitTypes::Enum::MAX == 234,
              "BWAPI 4.4 UnitTypes::MAX is not the expected ABI value");
static_assert(BWAPI::UpgradeTypes::Enum::MAX == 63,
              "BWAPI 4.4 UpgradeTypes::MAX is not the expected ABI value");
static_assert(BWAPI::TechTypes::Enum::MAX == 47,
              "BWAPI 4.4 TechTypes::MAX is not the expected ABI value");
static_assert(sizeof(BWAPI::PlayerData) == 5788,
              "BWAPI 4.4 PlayerData layout is not the expected ABI");
static_assert(sizeof(BWAPI::UnitData) == 336,
              "BWAPI 4.4 UnitData layout is not the expected ABI");
static_assert(sizeof(BWAPI::RegionData) == 1068,
              "BWAPI 4.4 RegionData layout is not the expected ABI");
static_assert(sizeof(BWAPI::GameData) == 33017048,
              "BWAPI 4.4 GameData layout is not the expected ABI");
static_assert(offsetof(BWAPI::GameInstance, serverProcessID) == 0);
static_assert(offsetof(BWAPI::GameInstance, isConnected) == 4);
static_assert(offsetof(BWAPI::GameInstance, lastKeepAliveTime) == 8);
static_assert(offsetof(BWAPI::GameData, client_version) == 0);
static_assert(offsetof(BWAPI::GameData, revision) == 4);
static_assert(offsetof(BWAPI::GameData, isDebug) == 8);
static_assert(offsetof(BWAPI::GameData, instanceID) == 12);
static_assert(offsetof(BWAPI::GameData, units) >
              offsetof(BWAPI::GameData, players));
static_assert(offsetof(BWAPI::GameData, initialUnitCount) == 69648);
static_assert(offsetof(BWAPI::GameData, units) == 69656);
static_assert(offsetof(BWAPI::GameData, unitCommands) >
              offsetof(BWAPI::GameData, commands));
static_assert(offsetof(BWAPI::GameData, eventCount) == 10586620);
static_assert(offsetof(BWAPI::GameData, strings) == 10962632);
static_assert(offsetof(BWAPI::GameData, unitCommands) == 32482644);
