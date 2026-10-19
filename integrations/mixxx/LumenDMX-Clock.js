// LumenDMX MIDI clock for Mixxx.
//
// Sends 24 ppqn MIDI clock that follows the phase of the deck LumenDMX should
// light to: the sync leader if one is set, otherwise the loudest playing deck.
// Pulses are derived from the deck's beat_distance rather than a free-running
// timer, so jog nudges, pitch bends and tempo changes all come through.
//
// Load it on the IAC Driver bus (see README.md) and set LumenDMX's tempo
// source to "MIDI clock" on the same bus.

var LumenClock = {};

LumenClock.PPQN = 24;
// MIDI Start is sent on a beat every this many beats. LumenDMX snaps its beat
// grid to the nearest whole beat on Start, which corrects any phase drift.
LumenClock.BEATS_PER_RESYNC = 4;
LumenClock.TICK_MS = 20;
LumenClock.DECKS = 4;
// A beat_distance jump bigger than this (that isn't a wrap past the beat) is
// a seek or a backwards scratch: wait for the next beat and resync.
LumenClock.SEEK_THRESHOLD = 0.25;

LumenClock.CLOCK = 0xF8;
LumenClock.START = 0xFA;
LumenClock.STOP = 0xFC;

LumenClock.init = function () {
    this.deck = null;
    this.lastDist = null;
    this.sent = 0; // pulses sent in the current beat
    this.beats = 0;
    this.synced = false; // a Start has been sent for the current source
    this.running = false;
    this.connections = [];
    for (var i = 1; i <= this.DECKS; i++) {
        // beat_distance changes once per audio buffer, which is finer than the
        // script timer; the timer covers decks stopping and source changes.
        this.connections.push(engine.makeConnection(this.group(i), "beat_distance", this.tick.bind(this)));
    }
    this.timer = engine.beginTimer(this.TICK_MS, this.tick.bind(this));
};

LumenClock.shutdown = function () {
    if (this.timer) {
        engine.stopTimer(this.timer);
    }
    this.connections.forEach(function (c) { c.disconnect(); });
    if (this.running) {
        this.send(this.STOP);
    }
};

LumenClock.group = function (n) {
    return "[Channel" + n + "]";
};

LumenClock.send = function (status) {
    midi.sendShortMsg(status, 0, 0);
};

// The sync leader if one is playing, else the playing deck with the highest
// channel fader, else null.
LumenClock.pickDeck = function () {
    var best = null;
    var bestVolume = -1;
    for (var i = 1; i <= this.DECKS; i++) {
        var g = this.group(i);
        if (!engine.getValue(g, "play")) {
            continue;
        }
        if (engine.getValue(g, "sync_leader") > 0) {
            return g;
        }
        var volume = engine.getValue(g, "volume");
        if (volume > bestVolume) {
            best = g;
            bestVolume = volume;
        }
    }
    return best;
};

LumenClock.tick = function () {
    var g = this.pickDeck();
    if (g === null) {
        if (this.running) {
            this.send(this.STOP);
            this.running = false;
        }
        this.deck = null;
        this.lastDist = null;
        return;
    }
    var dist = engine.getValue(g, "beat_distance");
    if (g !== this.deck || this.lastDist === null) {
        // New source: say nothing until its next beat, then Start on it.
        this.deck = g;
        this.lastDist = dist;
        this.synced = false;
        return;
    }
    var delta = dist - this.lastDist;
    this.lastDist = dist;
    if (delta < -0.5) {
        // Wrapped past a beat: finish the old beat's pulses, then begin the new one.
        this.beatStarted();
    } else if (Math.abs(delta) > this.SEEK_THRESHOLD) {
        this.synced = false;
        return;
    }
    if (!this.synced) {
        return;
    }
    // The beat's first pulse went out at the beat; the rest follow its phase.
    var due = Math.min(this.PPQN, Math.floor(dist * this.PPQN) + 1);
    while (this.sent < due) {
        this.send(this.CLOCK);
        this.sent++;
    }
};

LumenClock.beatStarted = function () {
    if (this.synced) {
        while (this.sent < this.PPQN) {
            this.send(this.CLOCK);
            this.sent++;
        }
        this.beats++;
    }
    if (!this.synced || this.beats % this.BEATS_PER_RESYNC === 0) {
        this.send(this.START);
        this.synced = true;
        this.running = true;
        this.beats = 0;
    }
    this.sent = 0;
};
