if(NOT WIN32)
    message(FATAL_ERROR "The legacy44 TournamentModule loader is a Windows Win32 target")
endif()

if(DEFINED MSBUILD_COMMAND AND NOT MSBUILD_COMMAND STREQUAL "")
    set(BWRAPI_MSBUILD_EXECUTABLE "${MSBUILD_COMMAND}")
else()
    find_program(BWRAPI_MSBUILD_EXECUTABLE NAMES MSBuild.exe msbuild)
endif()
if(NOT BWRAPI_MSBUILD_EXECUTABLE)
    message(FATAL_ERROR "MSBuild.exe is required to build the Win32 TournamentModule loader")
endif()

if(NOT DEFINED PLATFORM_TOOLSET OR PLATFORM_TOOLSET STREQUAL "")
    set(PLATFORM_TOOLSET "v145")
endif()

file(MAKE_DIRECTORY "${OUTPUT_DIR}")
set(_intermediate_dir "${OUTPUT_DIR}/intermediate")
file(MAKE_DIRECTORY "${_intermediate_dir}")

include("${CMAKE_CURRENT_LIST_DIR}/stage_legacy44_client.cmake")
bwrapi_stage_legacy44_client("${UPSTREAM_ROOT}" "${OUTPUT_DIR}/source" _client_project)
file(TO_NATIVE_PATH "${_client_project}" _client_project_native)
file(TO_NATIVE_PATH "${PROJECT_FILE}" _project_native)
file(TO_NATIVE_PATH "${UPSTREAM_ROOT}" _upstream_native)
file(TO_NATIVE_PATH "${OUTPUT_DIR}/" _output_dir_native)
file(TO_NATIVE_PATH "${_intermediate_dir}/" _intermediate_dir_native)

execute_process(
    COMMAND "${BWRAPI_MSBUILD_EXECUTABLE}" "${_project_native}"
        /t:Build
        /p:Configuration=Release
        /p:Platform=Win32
        "/p:PlatformToolset=${PLATFORM_TOOLSET}"
        "/p:UpstreamRoot=${_upstream_native}"
        "/p:BwrapiClientProject=${_client_project_native}"
        "/p:OutDir=${_output_dir_native}"
        "/p:IntDir=${_intermediate_dir_native}"
        /p:UseStructuredOutput=false
        /m
    RESULT_VARIABLE _build_result
    OUTPUT_VARIABLE _build_output
    ERROR_VARIABLE _build_error
)
if(NOT _build_result EQUAL 0)
    message(FATAL_ERROR
        "TournamentModule loader build failed:\n${_build_output}\n${_build_error}")
endif()

if(NOT EXISTS "${OUTPUT_EXE}")
    message(FATAL_ERROR
        "MSBuild succeeded but bwrapi-legacy44-tournament-loader.exe was not found")
endif()
