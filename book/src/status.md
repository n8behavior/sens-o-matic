# Status

The backend API covers users, groups, pings, responses and hangouts, the
lifecycle of a ping, and the matching of time overlaps. It keeps all its data
in memory, so a restart of the server loses the data: there is no persistence
layer yet.

The [specification](specification.md) describes more than the API implements.
The code has no natural-language understanding, no notifications, and no venue
scoring or venue vote.
