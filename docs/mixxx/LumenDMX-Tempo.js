// Sends Mixxx's tempo to LumenDMX (tempo source "Mixxx"). Output only.
//
// MIDI channel 16:
//   CC 20 then CC 52 : BPM x 10 as a 14-bit number (MSB, LSB)
//   note 0           : a beat (velocity 127 on, 0 off)
//
// The tempo follows the sync leader if there is one, otherwise the lowest
// numbered playing deck.

var LumenTempo = {};

LumenTempo.DECKS = 4;
LumenTempo.lastTenths = -1;

LumenTempo.activeDeck = function () {
    var firstPlaying = 0;
    for (var d = 1; d <= LumenTempo.DECKS; d++) {
        var g = "[Channel" + d + "]";
        if (!engine.getValue(g, "play")) { continue; }
        if (engine.getValue(g, "sync_leader") > 0) { return d; }
        if (!firstPlaying) { firstPlaying = d; }
    }
    return firstPlaying;
};

LumenTempo.sendBpm = function (force) {
    var d = LumenTempo.activeDeck();
    if (!d) { return; }
    var tenths = Math.round(engine.getValue("[Channel" + d + "]", "bpm") * 10);
    if (tenths <= 0 || tenths > 16383) { return; }
    if (!force && tenths === LumenTempo.lastTenths) { return; }
    LumenTempo.lastTenths = tenths;
    midi.sendShortMsg(0xBF, 20, tenths >> 7);
    midi.sendShortMsg(0xBF, 52, tenths & 0x7F);
};

LumenTempo.init = function () {
    for (var d = 1; d <= LumenTempo.DECKS; d++) {
        (function (deck) {
            var g = "[Channel" + deck + "]";
            engine.makeConnection(g, "beat_active", function (value) {
                if (deck !== LumenTempo.activeDeck()) { return; }
                midi.sendShortMsg(0x9F, 0, value ? 127 : 0);
            });
            engine.makeConnection(g, "bpm", function () { LumenTempo.sendBpm(false); });
            engine.makeConnection(g, "play", function () { LumenTempo.sendBpm(true); });
            engine.makeConnection(g, "sync_leader", function () { LumenTempo.sendBpm(true); });
        })(d);
    }
    // Re-send now and then so LumenDMX picks the tempo up if it starts later.
    LumenTempo.timer = engine.beginTimer(2000, function () { LumenTempo.sendBpm(true); });
    LumenTempo.sendBpm(true);
};

LumenTempo.shutdown = function () {
    engine.stopTimer(LumenTempo.timer);
};
