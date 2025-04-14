"""
This script reads a rerun file, decodes the images and logs them to rerun again.
"""

import argparse
from pathlib import Path
import kornia_rs as kr
import rerun as rr


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Read a rerun file and print the messages"
    )
    parser.add_argument("--log-file", type=Path, required=True)
    args = parser.parse_args()

    rr.init("rerun_video_example", spawn=True)

    # load the recording
    recording = rr.dataframe.load_recording(args.log_file)
    # print(recording.schema().component_columns())

    image_decoder = kr.ImageDecoder()

    for cam_topic in ["/cam/0", "/cam/1"]:
        print(f"Processing {cam_topic} ...")
        view = recording.view(index="log_time", contents=cam_topic)
        table = view.select().read_all()

        # convert the table to a pandas dataframe to iterate over the rows
        df = table.to_pandas()

        for _, row in df.iterrows():
            _, time, blob, media_type = row
            if media_type is None:
                continue

            # decode the jpeg image to a numpy array HxWxC
            image = image_decoder.decode(blob[0].tobytes())

            rr.set_time_nanos("timeline", time.nanosecond)
            rr.log(cam_topic, rr.Image(image))


if __name__ == "__main__":
    main()
