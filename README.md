# SteamMusicServer

Standalone app that finds all of your steam music.

## Building


## Configuration

### Scanner Config File:
`` config/scanner_conf.yaml`` 

See example file included.
Modify the media types you care about
include music directories from each steam library
include the steamapps directory if you want to find hidden music and sound effects.

### Get API key here:
Needs an API key because this will download the list of all games you own to help build the catalog of games with associated music.
https://steamcommunity.com/dev/apikeyexport 

### Environment Variables
Put these in your .bashrc
On windows you know where they go

    export STEAM_API_KEY="XXXXXXXXXXXXXXXX"
    export STEAM_ID="XXXXXXXXXXXXX"

## Usage:
steam_music_server --serve

this will scan the places configured in your scanner_Conf.yaml and build you a sqlite3 music database.  The starkeeper.db is refreshed and replaced on every scan.

player.db will persist and has the state of your library and additional meta data.

Go to the webpage it shows and you can start playing music.
This web page is meant to show that everything is working.
This app provides an API so that other music players may access this.

## Related Projects:
- none yet but tune in for a list of compatible players
- command line player!
- game showcase player!
- and more!!!
