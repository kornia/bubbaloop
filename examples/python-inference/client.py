"""Example of a client that requests the inference result from the server."""

import argparse
import asyncio
import httpx
import rerun as rr
import numpy as np
import kornia_rs as kr


async def get_api_response(client: httpx.AsyncClient, url: str) -> dict | None:
    try:
        response = await client.get(url)
    except httpx.HTTPError as _:
        print("The request timed out. Please try again.")
        return

    if response is None:
        return None

    json_response = response.json()
    return json_response


def response_to_image(response: dict) -> rr.Image:
    # decode the image
    decoder = kr.ImageDecoder()
    image_json = response["Success"]["image"]
    data = decoder.decode(bytes(image_json["data"]))
    return rr.Image(data)


def response_to_inference_result(response: dict) -> rr.Boxes2D:
    detections = response["Success"]["detections"]
    array = np.array([[d["xmin"], d["ymin"], d["xmax"], d["ymax"]] for d in detections])
    return rr.Boxes2D(
        array=array,
        array_format=rr.Box2DFormat.XYXY,
        class_ids=np.array([d["class"] for d in detections]),
    )


async def main() -> None:
    """Main function to receive the inference result from the server."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=str, default="0.0.0.0")
    parser.add_argument("--port", type=int, default=3000)
    args = parser.parse_args()

    rr.init("rerun_example_my_data", spawn=True)

    client = httpx.AsyncClient(timeout=None)

    while True:
        # get the image from the server
        response = await get_api_response(
            client,
            f"http://{args.host}:{args.port}/api/v0/inference/image",
        )
        if response is not None and "Success" in response:
            rr.set_time_sequence("session", response["Success"]["timestamp_nanos"])
            rr.log("/image", response_to_image(response))

        # get the inference result from the server
        response = await get_api_response(
            client, f"http://{args.host}:{args.port}/api/v0/inference/result"
        )
        if response is not None and "Success" in response:
            rr.set_time_sequence("session", response["Success"]["timestamp_nanos"])
            rr.log("/image/detections", response_to_inference_result(response))


if __name__ == "__main__":
    asyncio.run(main())
