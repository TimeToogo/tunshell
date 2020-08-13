import styled from "styled-components";

export const Hero = styled.div`
  padding: 15vh 0;

  h1 {
    font-weight: normal;
    font-size: 40px;
    line-height: 45px;
  }

  .typing {
    &:after {
      content: "_";
      animation: blink 1s infinite;
    }
  }

  p {
    font-size: 20px;
    line-height: 35px;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  @media (max-width: 768px) {
    padding: 30px 0;

    h1 {
      height: 135px;
    }
  }

  @media (max-width: 350px) {
    padding: 20px 0;

    h1 {
      height: 175px;
    }
  }
`;
