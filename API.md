# Online Radio Box API

## Country List

https://onlineradiobox.com/json

Returns the list of countries with radio stations available to stream. Data is in this JSON schema:

```
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "timeStamp": {
      "type": "integer",
      "examples": [
        1783068900
      ]
    },
    "countries": {
      "type": "array",
      "items": {
        "type": "string",
        "examples": [
          "li"
        ]
      },
      "examples": [
        ["li", "ki", "je"]
      ]
    },
    "cdn": {
      "type": "string",
      "examples": [
        "cdn.onlineradiobox.com"
      ]
    },
    "api": {
      "type": "string",
      "examples": [
        "onlineradiobox.com"
      ]
    },
    "trackServer": {
      "type": "string",
      "examples": [
        "https://scraper2.onlineradiobox.com/"
      ]
    },
    "renew": {
      "type": "boolean",
      "examples": [
        false
      ]
    }
  },
  "examples": [
    {
      "timeStamp": 1783068900,
      "countries": ["li", "is", "dm"],
      "cdn": "cdn.onlineradiobox.com",
      "api": "onlineradiobox.com",
      "trackServer": "https://scraper2.onlineradiobox.com/",
      "renew": false
    }
  ]
}
```

## Station List per Country

> NOTE: This misses out a lot of stations, so should be ignored for now

https://onlineradiobox.com/json/uk/


Returns the list of stations available to stream, for a specific two-letter country code. Returns data in the following JSON schema:

```
{
    "timeStamp": 1783067884,
    "stations": [{
            "id": 141087,
            "version": 2,
            "regionId": 6269131,
            "cityId": 6433,
            "cityName": "Birmingham",
            "alias": "fusionbirmingham",
            "title": "Fusion Radio",
            "rank": 9,
            "listeners": 0,
            "country": "uk",
            "status": 268447749,
            "genres": [
                "dance",
                "rock",
                "90s",
                "00s",
                "80s",
                "70s",
                "60s"
            ],
            "genreIds": [
                1,
                9,
                113,
                114,
                116,
                122,
                136
            ],
            "catIds": [
                4,
                3,
                2,
                8
            ]
        }]
}
```

## Station Information

https://onlineradiobox.com/json/$country/$alias

Returns the full information for a station, given its `country` and `alias`. Returns JSON data following this JSON schema:

```
{"timeStamp":1783067818,"station":{"id":97780,"version":25,"groupId":1185,"regionId":6269131,"cityId":4254,"cityName":"London","alias":"bbcdance","title":"BBC Radio 1 Dance","rank":493,"listeners":14,"country":"uk","status":0,"genres":["dance"],"genreIds":[1],"catIds":[4],"description":"BBC Radio 1 Dance is a national digital radio station in the United Kingdom, owned and operated by the BBC and run as a spin-off from BBC Radio 1. The station plays a mix of back-to-back current, future and classic electronic dance music, and broadcasts exclusively on BBC Sounds."}}

```

## Station Now Playing

http://scraper.onlineradiobox.com/$country.$alias

Returns the currently playing song given a station's `country` and `alias`. Returns data with the following JSON schema:

```
{"alias":"uk.bbcradio1","stationId":1193,"updated":1783067791,"trackId":"1369276698548314001","title":"Aitch - Rain (feat. Tay Keith)","citatisId":40608,"iName":"Rain (feat. Tay Keith)","iArtist":"AJ Tracey","iImg":"https://is5-ssl.mzstatic.com/image/thumb/Music114/v4/81/10/f1/8110f16c-e808-aa3c-46b6-c060c0c3e248/20UMGIM16505.rgb.jpg/360x360bb.jpg"}
```

## Station Stream Information

https://onlineradiobox.com/json/$country/$alias/widget/

Returns the station's stream information, given the station's `country` and `alias`. Returns data with the following JSON schema:

```
{"streamURL":"https://a.files.bbci.co.uk/ms6/live/3441A116-B12E-4D2F-ACA8-C1984642FA4B/audio/simulcast/dash/nonuk/pc_hd_abr_v2/cfs/bbc_radio_one_dance.mpd","streamType":6,"isGeoBlocked":false,"isRestricted":false}

```
