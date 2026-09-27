# Project goal

The goal of the project is to create an application to create annotations ( adding text, shapes, ... ) on a video.


# Project detail

The application should be able to load a video and save the annotated videos in various format ( gif included ).
The application should have a video player with some ways - buttons and shortcuts - to move in the video, like +/-1 sec, +/- 5 secs, +/- 1 frame. The player should have a full screen option.
Under the video player, there should be a timeline to define the lifetime of the annotations.
This timeline also enable the user to add a new entry, and so a new annotation.
Annotations should be for now : text, rectangle, ellipsis, arrows.
Each annotations could have some specific effects/animations ( like for instance a moving effect on a rectangle like a loading spinner on YouTube, or a glowing effect ).
Annotations can have fade in / fade out times.

Every thing should be in english in the application.


#implementation 

The application should be done in Rust, it can use some external libraries like FFMPEG if needed.

Each project contains a single source video. Multi-clip editing is outside the scope of this project.
