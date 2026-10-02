//! Every piece of FATHOM's writing: the bestiary, the Meridian logs, the intro and the endings.

use super::world::Species;

pub struct Codex {
    pub name: &'static str,
    pub glyph: &'static str,
    pub text: &'static str,
    pub hint: &'static str,
}

pub fn codex(s: Species) -> Codex {
    match s {
        Species::Glimmer => Codex {
            name: "Glimmer shoal",
            glyph: "·",
            text: "Lanternfish, loosely: thousands of photophores blinking in slow waves. They scatter from any sound and re-form behind you.",
            hint: "Shoals flee toward silence. Follow them and you'll find open water nobody is hunting in.",
        },
        Species::Jelly => Codex {
            name: "Drift jelly",
            glyph: "Ω",
            text: "A siphonophore colony a metre across that flares violet when struck by sound. Its sting carries a charge that drains anything electrical.",
            hint: "Jellies light up for a few seconds when pinged, so one ping shows every jelly nearby.",
        },
        Species::Gulper => Codex {
            name: "Gulper",
            glyph: "Θ",
            text: "An ambush feeder whose jaw is a third of its body. It will wait for days without moving, then strike in a fraction of a second.",
            hint: "Gulpers never chase. Ping before you hug a wall, and never park next to one.",
        },
        Species::Angler => Codex {
            name: "Beacon angler",
            glyph: "Ψ",
            text: "Its lure mimics the amber blink of an emergency beacon so well that only the rhythm gives it away.",
            hint: "Wreck beacons blink evenly. Lures flicker twice. The Lure Filter research colours them red.",
        },
        Species::Eel => Codex {
            name: "Hunter eel",
            glyph: "ξ",
            text: "Blind. Its whole body is an ear. It swims to where a sound was, not to where the sound is now.",
            hint: "Ping, then move. A decoy keeps every eel in earshot busy for eight seconds.",
        },
        Species::Leviathan => Codex {
            name: "Leviathan",
            glyph: "█",
            text: "Too large to see whole. Its breathing is a slow pressure you feel through the hull. It sleeps, mostly, and every loud noise nudges it closer to waking.",
            hint: "In the Hadal zone, go dark. Trust your chart, and save your pings for when you're lost.",
        },
    }
}

pub struct Log {
    pub author: &'static str,
    pub depth: &'static str,
    pub text: &'static str,
}

pub const LOGS: [Log; 12] = [
    Log {
        author: "Ana Okafor, biologist",
        depth: "300 m",
        text: "Day one. Past the shelf and into the dark. Ilse wants active sonar kept to a minimum. 'Listen first,' she says. Tomas thinks she's being superstitious. I think she's being a scientist.",
    },
    Log {
        author: "Tomas Reyes, pilot",
        depth: "1,100 m",
        text: "Kill the floods and there are lanterns everywhere. Turn them on and nothing. Like the whole trench holds its breath until we look away.",
    },
    Log {
        author: "Dr. Ilse Varga, acoustician",
        depth: "1,900 m",
        text: "A low pulse from below, under one hertz. Regular. Too regular. I'm calling it the metronome until I know what it is.",
    },
    Log {
        author: "Ana Okafor, biologist",
        depth: "2,700 m",
        text: "Tagged a gulper that didn't move for nine hours. Then it moved, and Tomas had to put us into the wall to get clear. The hull is fine. Tomas is not.",
    },
    Log {
        author: "Dr. Ilse Varga, acoustician",
        depth: "3,500 m",
        text: "The metronome changed tempo when we pinged. It waited exactly as long as our echo takes to come back. Then it answered.",
    },
    Log {
        author: "Tomas Reyes, pilot",
        depth: "4,300 m",
        text: "The eels don't see us. They hear us. Ilse worked it out: they go where the noise was, not where we are. Running silent from now on.",
    },
    Log {
        author: "Ana Okafor, biologist",
        depth: "5,100 m",
        text: "Vents. Warm water, white crabs, life everywhere. If we never went home I think I'd be alright. Ilse hasn't slept in two days.",
    },
    Log {
        author: "Dr. Ilse Varga, acoustician",
        depth: "6,000 m",
        text: "Sonar returned a wall where there is no wall. I checked three times. The echo came back before our pulse could have arrived. Something is pinging us with our own signature.",
    },
    Log {
        author: "Tomas Reyes, pilot",
        depth: "6,900 m",
        text: "Something the size of a ship turned over in the dark below us. We cut everything and drifted. It went back to sleep. I think.",
    },
    Log {
        author: "Dr. Ilse Varga, acoustician",
        depth: "7,800 m",
        text: "It isn't hunting. It's calling. The song is our ping, slowed down and layered on itself, over and over. It learned our voice. It wants an answer.",
    },
    Log {
        author: "Ana Okafor, biologist",
        depth: "8,900 m",
        text: "Tomas wants to surface. Ilse wants to reply. I want to know what it is. We put it to a vote. Two to one. We're going down.",
    },
    Log {
        author: "Dr. Ilse Varga, acoustician",
        depth: "10,200 m",
        text: "If anyone finds this: we answered. It was not angry. It was so alone. Tell Tomas's daughter he was brave. We're going to see the floor.",
    },
];

pub const INTRO: [&str; 5] = [
    "1994. The research submersible MERIDIAN descended into the Tern Trench with a crew of three. She was never heard from again.",
    "Thirty years later, you've been hired to chart that trench.",
    "Your sub has a lamp, a sonar array and a hydrophone. Down there, light is scarce and sound carries a long way.",
    "Every ping lights up the dark for a few seconds. It also tells everything nearby exactly where you are.",
    "Bring back what you find. Go deeper each time. Find out what happened to the Meridian.",
];

pub const FLOOR_ARRIVAL: &str = "The Meridian lies on her side in the silt, hatch open. Your hydrophone fills with a slow, layered pulse. It is your own ping, sung back to you, again and again. It is waiting for a reply.";

pub const ENDING_ANSWER: [&str; 4] = [
    "You ping in the Song's rhythm. For a long moment there is nothing.",
    "Then the whole trench lights up, not with light but with echo: every wall, every creature, every hollow for a thousand metres, drawn in sound. A gift.",
    "Inside the Meridian you find a tape. Three voices laughing, and under them, the Song, answering.",
    "You surface with the recording. The trench is quieter now. Not empty. Listening.",
];

pub const ENDING_DARK: [&str; 4] = [
    "You kill the lamp. You stop the engine. You drift to the Meridian in silence and cut her black box free.",
    "Above you the Song falters, rises, searches.",
    "You climb for hours with every instrument dark.",
    "At the surface the hydrophone catches one last pulse from far below: your own ping, perfect, repeated once. Goodbye. Or see you soon.",
];

pub const TIPS: [&str; 8] = [
    "Echoes show where things WERE. A ghost on your screen is a picture of the past.",
    "Eels go where the noise was. Ping, then move away from where you pinged.",
    "Real wreck beacons blink evenly. Angler lures flicker twice.",
    "Below your hull's rating the pressure starts to crush it. Upgrade the hull to go deeper.",
    "Cargo only counts once you surface. Knowledge is transmitted the moment you find it.",
    "Press r to autopilot back up the way you came. Any key cancels it.",
    "Jellies flare when pinged, and their sting drains your battery.",
    "Vents are hot to touch, but sitting beside one slowly recharges your battery.",
];
