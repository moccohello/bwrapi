cmake_minimum_required(VERSION 3.20)

project(BWRAPIBWAPI VERSION 1.0.0 LANGUAGES CXX)

include(CMakePackageConfigHelpers)
include(CTest)
include(FetchContent)

set(BWRAPI_REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/.."
    CACHE PATH "BWRAPI repository root")

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(MSVC)
    add_compile_options(/utf-8)
endif()

# The pinned upstream 4.4 tree is fetched by configure directly from the
# fixed commit; a local mirror can be supplied with the standard
# FETCHCONTENT_SOURCE_DIR_BWAPI override.
FetchContent_Declare(BWAPI
    GIT_REPOSITORY https://github.com/bwapi/bwapi.git
    GIT_TAG d727fed68558c506163048ea889131d8cbb33915)
FetchContent_MakeAvailable(BWAPI)
set(BWRAPI_BWAPI_SOURCE "${bwapi_SOURCE_DIR}/bwapi")
set(BWRAPI_BWAPI44_INCLUDE_DIR
    "${BWRAPI_BWAPI_SOURCE}/include")
if(NOT EXISTS "${BWRAPI_BWAPI44_INCLUDE_DIR}/BWAPI.h")
    message(FATAL_ERROR
        "Pinned BWAPI 4.4 headers were not found under ${BWRAPI_BWAPI44_INCLUDE_DIR}")
endif()

# The BWAPI server is a DLL injected into the game; an x64 configure builds
# the one for the x64 game and a Win32 configure the one for the x86 game.
# The layout-check object is a common WIN32 target so both configures compile
# the same headers and static_asserts; they catch a Client header layout change.

if(WIN32)
    add_library(BWRAPIBWAPILegacy44LayoutCheck OBJECT
        runtime/legacy44-host/legacy44_layout_check.cpp
    )
    target_include_directories(BWRAPIBWAPILegacy44LayoutCheck PRIVATE
        "${BWRAPI_BWAPI44_INCLUDE_DIR}"
    )
    if(MSVC)
        target_compile_options(BWRAPIBWAPILegacy44LayoutCheck PRIVATE /utf-8)
    endif()
    add_custom_target(BWRAPIBWAPILegacy44LayoutCheckCompile ALL
        DEPENDS BWRAPIBWAPILegacy44LayoutCheck
    )
endif()

# The public bwrapi C ABI 1.0: generated header and import library of
# bwrapi.dll of the configured pointer width.  `cargo xtask build` writes
# both and passes their paths; the defaults match a build directory laid out
# like the repository's `build/`, where cargo puts the x86 build under its
# target triple.
if(CMAKE_SIZEOF_VOID_P EQUAL 4)
    set(BWRAPI_ABI_DEFAULT_LIBRARY_DIR "${BWRAPI_REPO_ROOT}/build/cargo/i686-pc-windows-msvc/release")
else()
    set(BWRAPI_ABI_DEFAULT_LIBRARY_DIR "${BWRAPI_REPO_ROOT}/build/cargo/release")
endif()
set(BWRAPI_ABI_INCLUDE_DIR "${BWRAPI_REPO_ROOT}/build/bindings"
    CACHE PATH "Directory with the generated bwrapi.h")
set(BWRAPI_ABI_LIBRARY "${BWRAPI_ABI_DEFAULT_LIBRARY_DIR}/bwrapi.dll.lib"
    CACHE FILEPATH "Import library of bwrapi.dll")
if(NOT EXISTS "${BWRAPI_ABI_INCLUDE_DIR}/bwrapi.h")
    message(FATAL_ERROR
        "bwrapi.h was not found under ${BWRAPI_ABI_INCLUDE_DIR}; run cargo xtask build or set BWRAPI_ABI_INCLUDE_DIR")
endif()
get_filename_component(BWRAPI_ABI_LIBRARY_DIR "${BWRAPI_ABI_LIBRARY}" DIRECTORY)
set(BWRAPI_ABI_DLL "${BWRAPI_ABI_LIBRARY_DIR}/bwrapi.dll")
# Built with bwrapi.dll; injects games of this width for an injector of the
# other width.
set(BWRAPI_INJECT_EXE "${BWRAPI_ABI_LIBRARY_DIR}/bwrapi-inject.exe")

# Sources of the GameData projection; the unit tests build them without the
# game or bwrapi.dll.
set(BWRAPI_HOST_PROJECTION_SOURCES
    runtime/legacy44-host/legacy44_common.cpp
    runtime/legacy44-host/legacy44_metrics.cpp
    runtime/legacy44-host/legacy44_projection.cpp
    runtime/legacy44-host/legacy44_projection_units.cpp
    runtime/legacy44-host/legacy44_projection_players.cpp
    runtime/legacy44-host/legacy44_projection_map.cpp
    # Unit, tech and upgrade prices and build times of the pinned BWAPILIB,
    # for the repair and refund statistics.
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/UnitType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/TechType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/UpgradeType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/Race.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/WeaponType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/UnitSizeType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/DamageType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/ExplosionType.cpp"
    "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/Order.cpp"
)

if(WIN32)
    # The in-game BWAPI server (GameTable, GameData, pipe, rounds), injected
    # into the game beside bwrapi.dll. It links no import library: it
    # resolves the ABI from the bwrapi.dll the game already loaded. The
    # static CRT keeps the game free of a C++ runtime dependency. Draw shapes
    # take their palette colors from the pinned BWAPILIB Color source.
    add_library(BWRAPIBWAPILegacy44Server SHARED
        runtime/legacy44-host/legacy44_game_server.cpp
        runtime/legacy44-host/legacy44_draw.cpp
        runtime/legacy44-host/legacy44_server.cpp
        runtime/legacy44-host/legacy44_transport.cpp
        runtime/legacy44-host/legacy44_events.cpp
        runtime/legacy44-host/legacy44_chat.cpp
        runtime/legacy44-host/legacy44_command.cpp
        runtime/legacy44-host/legacy44_latcom.cpp
        runtime/legacy44-host/legacy44_apm.cpp
        ${BWRAPI_HOST_PROJECTION_SOURCES}
        "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/Color.cpp"
        # Exports the hook entry undecorated on x86 too, where a stdcall
        # dllexport would be named _bwrapi_bwapi_callwndproc@12.
        runtime/legacy44-host/legacy44_game_server.def
    )
    set_target_properties(BWRAPIBWAPILegacy44Server PROPERTIES
        OUTPUT_NAME bwrapi_bwapi
        MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>"
    )
    target_include_directories(BWRAPIBWAPILegacy44Server PRIVATE
        "${BWRAPI_ABI_INCLUDE_DIR}"
        "${BWRAPI_BWAPI_SOURCE}/include"
    )
    if(MSVC)
        target_compile_options(BWRAPIBWAPILegacy44Server PRIVATE /utf-8)
    endif()
    # The injector loads bwrapi.dll from the server DLL's directory first,
    # and runs the bwrapi-inject.exe there for a game of the other width.
    add_custom_command(TARGET BWRAPIBWAPILegacy44Server POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${BWRAPI_ABI_DLL}" "${BWRAPI_INJECT_EXE}"
            "$<TARGET_FILE_DIR:BWRAPIBWAPILegacy44Server>"
        VERBATIM
    )
    install(TARGETS BWRAPIBWAPILegacy44Server
        RUNTIME DESTINATION bin
    )
    install(FILES "${BWRAPI_ABI_DLL}" "${BWRAPI_INJECT_EXE}" DESTINATION bin)
    install(FILES "${BWRAPI_ABI_INCLUDE_DIR}/bwrapi.h" DESTINATION include/BWRAPI)
endif()

if(WIN32 AND CMAKE_SIZEOF_VOID_P EQUAL 8)
    # The loaders are x86 programs built by MSBuild for Win32 whatever this
    # configure targets, so only the x64 configure builds them. They are
    # retargeted from the upstream projects to the toolset of the Visual
    # Studio that configures this build.
    set(BWRAPI_BWAPI_LEGACY44_PLATFORM_TOOLSET
        "${CMAKE_VS_PLATFORM_TOOLSET}"
        CACHE STRING "MSVC toolset used to retarget the pinned BWAPI loader")

    set(BWRAPI_BWAPI_LEGACY44_LOADER_DIR
        "${CMAKE_CURRENT_BINARY_DIR}/legacy44-loader")
    set(BWRAPI_BWAPI_LEGACY44_LOADER_EXE
        "${BWRAPI_BWAPI_LEGACY44_LOADER_DIR}/bwrapi-legacy44-loader.exe")
    add_custom_command(
        OUTPUT "${BWRAPI_BWAPI_LEGACY44_LOADER_EXE}"
        COMMAND "${CMAKE_COMMAND}"
            "-DUPSTREAM_ROOT=${BWRAPI_BWAPI_SOURCE}"
            "-DOUTPUT_DIR=${BWRAPI_BWAPI_LEGACY44_LOADER_DIR}"
            "-DOUTPUT_EXE=${BWRAPI_BWAPI_LEGACY44_LOADER_EXE}"
            "-DMSBUILD_COMMAND=${CMAKE_VS_MSBUILD_COMMAND}"
            "-DPLATFORM_TOOLSET=${BWRAPI_BWAPI_LEGACY44_PLATFORM_TOOLSET}"
            "-P" "${CMAKE_CURRENT_SOURCE_DIR}/cmake/build_legacy44_loader.cmake"
        DEPENDS
            "${CMAKE_CURRENT_SOURCE_DIR}/cmake/build_legacy44_loader.cmake"
            "${CMAKE_CURRENT_SOURCE_DIR}/cmake/stage_legacy44_client.cmake"
            "${BWRAPI_BWAPI_SOURCE}/AIModuleLoader/Source/AIModuleLoader.cpp"
            "${BWRAPI_BWAPI_SOURCE}/AIModuleLoader/AIModuleLoader.vcxproj"
            "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/BWAPILIB.vcxproj"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/BWAPIClient.vcxproj"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/Source/GameImpl.cpp"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/Source/Client.cpp"
            "${BWRAPI_BWAPI_SOURCE}/Shared/GameShared.cpp"
            "${BWRAPI_BWAPI_SOURCE}/Shared/BulletShared.cpp"
        VERBATIM
    )
    add_custom_target(BWRAPIBWAPILegacy44Loader ALL
        DEPENDS "${BWRAPI_BWAPI_LEGACY44_LOADER_EXE}"
    )
    install(FILES "${BWRAPI_BWAPI_LEGACY44_LOADER_EXE}"
        DESTINATION bin
    )
    # BWAPILIB.lib is produced by the same MSBuild project-reference build
    # as BWAPIClient.lib, not from a prebuilt upstream artifact.
    install(FILES "${BWRAPI_BWAPI_LEGACY44_LOADER_DIR}/BWAPIClient.lib"
        "${BWRAPI_BWAPI_LEGACY44_LOADER_DIR}/BWAPILIB.lib"
        DESTINATION lib
    )

    set(BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_DIR
        "${CMAKE_CURRENT_BINARY_DIR}/legacy44-tournament-loader")
    set(BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_EXE
        "${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_DIR}/bwrapi-legacy44-tournament-loader.exe")
    set(BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_PROJECT
        "${CMAKE_CURRENT_SOURCE_DIR}/runtime/legacy44-tournament-loader/legacy44-tournament-loader.vcxproj")
    add_custom_command(
        OUTPUT "${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_EXE}"
        COMMAND "${CMAKE_COMMAND}"
            "-DPROJECT_FILE=${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_PROJECT}"
            "-DUPSTREAM_ROOT=${BWRAPI_BWAPI_SOURCE}"
            "-DOUTPUT_DIR=${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_DIR}"
            "-DOUTPUT_EXE=${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_EXE}"
            "-DMSBUILD_COMMAND=${CMAKE_VS_MSBUILD_COMMAND}"
            "-DPLATFORM_TOOLSET=${BWRAPI_BWAPI_LEGACY44_PLATFORM_TOOLSET}"
            "-P" "${CMAKE_CURRENT_SOURCE_DIR}/cmake/build_legacy44_tournament_loader.cmake"
        DEPENDS
            "${CMAKE_CURRENT_SOURCE_DIR}/cmake/build_legacy44_tournament_loader.cmake"
            "${CMAKE_CURRENT_SOURCE_DIR}/cmake/stage_legacy44_client.cmake"
            "${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_PROJECT}"
            "${CMAKE_CURRENT_SOURCE_DIR}/runtime/legacy44-tournament-loader/main.cpp"
            "${CMAKE_CURRENT_SOURCE_DIR}/runtime/legacy44-tournament-loader/tournament_policy.cpp"
            "${CMAKE_CURRENT_SOURCE_DIR}/runtime/legacy44-tournament-loader/tournament_policy.h"
            "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/BWAPILIB.vcxproj"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/BWAPIClient.vcxproj"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/Source/GameImpl.cpp"
            "${BWRAPI_BWAPI_SOURCE}/BWAPIClient/Source/Client.cpp"
            "${BWRAPI_BWAPI_SOURCE}/Shared/GameShared.cpp"
            "${BWRAPI_BWAPI_SOURCE}/Shared/BulletShared.cpp"
        VERBATIM
    )
    add_custom_target(BWRAPIBWAPILegacy44TournamentLoader ALL
        DEPENDS "${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_EXE}"
    )
    install(FILES "${BWRAPI_BWAPI_LEGACY44_TOURNAMENT_LOADER_EXE}"
        DESTINATION bin
    )
endif()

if(BUILD_TESTING)
    # Tests that build GameData from in-memory bwrapi frames (no game, no DLL).
    # The frame fixture follows the generated bwrapi.h, so it is generated too.
    find_package(Python3 REQUIRED COMPONENTS Interpreter)
    set(BWRAPI_FRAME_FIXTURE_DIR "${CMAKE_CURRENT_BINARY_DIR}/frame-fixture")
    add_custom_command(
        OUTPUT "${BWRAPI_FRAME_FIXTURE_DIR}/legacy44_frame_fixture.h"
        COMMAND "${CMAKE_COMMAND}" -E make_directory "${BWRAPI_FRAME_FIXTURE_DIR}"
        COMMAND Python3::Interpreter "${CMAKE_CURRENT_SOURCE_DIR}/tests/frame/gen_frame_fixture.py"
            "${BWRAPI_ABI_INCLUDE_DIR}/bwrapi.h" "${BWRAPI_FRAME_FIXTURE_DIR}/legacy44_frame_fixture.h"
        DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/tests/frame/gen_frame_fixture.py" "${BWRAPI_ABI_INCLUDE_DIR}/bwrapi.h"
        VERBATIM
    )
    add_custom_target(BWRAPIBWAPIFrameFixture DEPENDS "${BWRAPI_FRAME_FIXTURE_DIR}/legacy44_frame_fixture.h")

    function(bwrapi_host_projection_test name)
        add_executable(${name} EXCLUDE_FROM_ALL ${ARGN})
        add_dependencies(${name} BWRAPIBWAPIFrameFixture)
        target_include_directories(${name} PRIVATE
            "${BWRAPI_ABI_INCLUDE_DIR}"
            "${BWRAPI_BWAPI44_INCLUDE_DIR}"
            "${CMAKE_CURRENT_SOURCE_DIR}/tests/frame"
            "${BWRAPI_FRAME_FIXTURE_DIR}"
        )
        if(MSVC)
            target_compile_options(${name} PRIVATE /utf-8)
        endif()
        add_test(NAME ${name} COMMAND ${name})
    endfunction()

    if(WIN32)
        if(CMAKE_SIZEOF_VOID_P EQUAL 8)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44ProjectionUnitsTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_projection_units_test.cpp)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44ProjectionPlayersTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_projection_players_test.cpp)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44ProjectionMapTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_projection_map_test.cpp)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44UnitEventsTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_events.cpp
                runtime/legacy44-host/legacy44_unit_events_test.cpp)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44CommandTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_chat.cpp
                runtime/legacy44-host/legacy44_command.cpp
                runtime/legacy44-host/legacy44_command_test.cpp)
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44DrawTest
                runtime/legacy44-host/legacy44_draw.cpp
                runtime/legacy44-host/legacy44_draw_test.cpp
                "${BWRAPI_BWAPI_SOURCE}/BWAPILIB/Source/Color.cpp")
            bwrapi_host_projection_test(BWRAPIBWAPILegacy44LatcomTest
                ${BWRAPI_HOST_PROJECTION_SOURCES}
                runtime/legacy44-host/legacy44_latcom.cpp
                runtime/legacy44-host/legacy44_latcom_test.cpp)
        endif()
    endif()
    add_executable(BWRAPIBWAPILegacy44SplitTilesTest EXCLUDE_FROM_ALL
        runtime/legacy44-host/legacy44_split_tiles_test.cpp
    )
    add_test(NAME BWRAPIBWAPILegacy44SplitTilesTest
        COMMAND BWRAPIBWAPILegacy44SplitTilesTest)
    add_executable(BWRAPIBWAPILegacy44MetricsTest EXCLUDE_FROM_ALL
        runtime/legacy44-host/legacy44_metrics.cpp
        runtime/legacy44-host/legacy44_metrics_test.cpp
    )
    add_executable(BWRAPIBWAPILegacy44ApmTest EXCLUDE_FROM_ALL
        runtime/legacy44-host/legacy44_apm.cpp
        runtime/legacy44-host/legacy44_apm_test.cpp
    )
    add_test(NAME BWRAPIBWAPILegacy44ApmTest COMMAND BWRAPIBWAPILegacy44ApmTest)
    add_test(NAME BWRAPIBWAPILegacy44MetricsTest
        COMMAND BWRAPIBWAPILegacy44MetricsTest)
endif()

install(DIRECTORY "${BWRAPI_BWAPI44_INCLUDE_DIR}/"
    DESTINATION include
)
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../profiles/compatibility-profiles.json"
    DESTINATION share/BWRAPIBWAPI/profiles
)
# The launcher ships inside the bwrapi package (python -m bwrapi.launcher).
install(DIRECTORY "${BWRAPI_REPO_ROOT}/python/bwrapi/"
    DESTINATION share/BWRAPIBWAPI/python/bwrapi
    PATTERN "__pycache__" EXCLUDE
    PATTERN "*.pyc" EXCLUDE
)
configure_package_config_file(
    cmake/BWRAPIBWAPIConfig.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/BWRAPIBWAPIConfig.cmake"
    INSTALL_DESTINATION lib/cmake/BWRAPIBWAPI
)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/BWRAPIBWAPIConfigVersion.cmake"
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion
    ARCH_INDEPENDENT
)

install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/BWRAPIBWAPIConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/BWRAPIBWAPIConfigVersion.cmake"
    DESTINATION lib/cmake/BWRAPIBWAPI
)

