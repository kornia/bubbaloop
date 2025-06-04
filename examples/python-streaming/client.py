"""Example of a client that requests the streaming image from the server.

Usage:
    python examples/python-streaming/client.py --host 0.0.0.0 --port 3000 --cameras 0 1
"""

import argparse
import asyncio
import httpx
import rerun as rr
import kornia_rs as kr


async def get_api_response(client: httpx.AsyncClient, url: str) -> dict | None:
    try:
        response = await client.get(url)
    except httpx.HTTPError as _:
        print("The request timed out. Please try again.")
        return

    if response is None:
        return None

    return response.content


async def poll_image(client: httpx.AsyncClient, url: str, rr, camera_id: int):
    decoder = kr.ImageDecoder()
    while True:
        # get the image from the server
        response = await get_api_response(client, url)
        rr.log(f"/cam/{camera_id}", rr.Image(decoder.decode(response)))


async def main() -> None:
    """Main function to receive the streaming images from the server."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=str, default="0.0.0.0")
    parser.add_argument("--port", type=int, default=3000)
    parser.add_argument("--cameras", type=int, nargs="+", default=[0])
    args = parser.parse_args()

    rr.init("rerun_streaming_client", spawn=True)

    async with httpx.AsyncClient(timeout=None) as client:
        image_tasks = []
        for camera_id in args.cameras:
            image_tasks.append(
                asyncio.create_task(
                    poll_image(
                        client,
                        url=f"http://{args.host}:{args.port}/api/v0/streaming/image/{camera_id}",
                        rr=rr,
                        camera_id=camera_id,
                    )
                )
            )
        await asyncio.gather(*image_tasks)


if __name__ == "__main__":
    asyncio.run(main())