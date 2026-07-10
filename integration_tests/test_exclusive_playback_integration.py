#!/usr/bin/env python3
"""
Exclusive playback integration tests.
"""

import venv_bootstrap


def test_exclusive_playback_transitions_across_all_players(
    activemonitor_server,
    exclusive_playback_transition_sequence,
):
    """Ensure only the active player transitions while others stay paused/stopped."""
    result = exclusive_playback_transition_sequence(
        activemonitor_server,
        inactive_states=["paused", "stopped"],
        active_transition_states=["playing", "paused", "playing", "stopped"],
    )

    players = result["players"]
    sequences = result["sequences"]
    allowed_inactive_states = set(result["allowed_inactive_states"])
    transition_states = result["active_transition_states"]

    assert len(sequences) == len(players)

    for sequence in sequences:
        active_player_id = sequence["active_player_id"]
        steps = sequence["steps"]

        assert len(steps) == len(transition_states)

        for idx, step in enumerate(steps):
            expected_active_state = transition_states[idx]
            states = step["states"]
            playing_players = step["playing_players"]

            assert step["active_state"] == expected_active_state
            assert states[active_player_id] == expected_active_state

            for player_id, state in states.items():
                if player_id == active_player_id:
                    continue
                assert state in allowed_inactive_states

            if expected_active_state == "playing":
                assert playing_players == [active_player_id]
            else:
                assert active_player_id not in playing_players

        # final sequence state should be the last active transition state
        assert sequence["states"][active_player_id] == transition_states[-1]
